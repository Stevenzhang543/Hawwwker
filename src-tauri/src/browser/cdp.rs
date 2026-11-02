//! CDP 连接：同一条浏览器级连接上用 flatten session 复用所有标签页。
//! 两种传输：
//! - **pipe**（默认）：`--remote-debugging-pipe`，子进程 fd 3 读命令、fd 4 写响应，消息以 `\0` 分隔。不开任何端口，
//!   本机其他程序无法接入用户的登录会话（修复 v26.01 遗留：调试端口暴露）。
//! - **websocket**：`--remote-debugging-port` 的回退方案（管道启动失败时）。
//!
//! 读取侧只有一个分发者：按 id 投递响应、按 sessionId 广播事件（修复 v3.4 并发 recv 冲突）。

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>;

pub struct Conn {
    tx: mpsc::UnboundedSender<String>,
    pending: Pending,
    events: broadcast::Sender<Value>,
    next: AtomicU64,
    closed: Arc<AtomicBool>,
    pub transport: &'static str,
}

struct Dispatch {
    pending: Pending,
    events: broadcast::Sender<Value>,
    closed: Arc<AtomicBool>,
}

impl Dispatch {
    fn handle(&self, text: &str) {
        let Ok(v) = serde_json::from_str::<Value>(text) else { return };
        if let Some(id) = v.get("id").and_then(|x| x.as_u64()) {
            let waiter = self.pending.lock().ok().and_then(|mut m| m.remove(&id));
            if let Some(w) = waiter {
                let res = match v.get("error") {
                    Some(err) => Err(err.get("message").and_then(|m| m.as_str()).unwrap_or("CDP 错误").to_string()),
                    None => Ok(v.get("result").cloned().unwrap_or(Value::Null)),
                };
                let _ = w.send(res);
            }
        } else if v.get("method").is_some() {
            let _ = self.events.send(v);
        }
    }

    fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        if let Ok(mut m) = self.pending.lock() {
            for (_, w) in m.drain() {
                let _ = w.send(Err("浏览器连接已断开".into()));
            }
        }
    }
}

impl Conn {
    fn parts(transport: &'static str) -> (Conn, Dispatch, mpsc::UnboundedReceiver<String>) {
        let (tx, rx) = mpsc::unbounded_channel::<String>();
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let (events, _) = broadcast::channel(512);
        let closed = Arc::new(AtomicBool::new(false));
        let d = Dispatch { pending: pending.clone(), events: events.clone(), closed: closed.clone() };
        (Conn { tx, pending, events, next: AtomicU64::new(1), closed, transport }, d, rx)
    }

    /// websocket 传输（端口模式）。
    pub async fn connect_ws(ws_url: &str) -> Result<Arc<Conn>, String> {
        let (ws, _) = tokio::time::timeout(Duration::from_secs(15), tokio_tungstenite::connect_async(ws_url))
            .await
            .map_err(|_| "连接浏览器调试通道超时".to_string())?
            .map_err(|e| format!("连接浏览器调试通道失败：{e}"))?;
        let (mut sink, mut stream) = ws.split();
        let (conn, d, mut rx) = Conn::parts("port");
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                if sink.send(Message::Text(msg.into())).await.is_err() {
                    break;
                }
            }
            let _ = sink.close().await;
        });
        tokio::spawn(async move {
            while let Some(msg) = stream.next().await {
                match msg {
                    Ok(Message::Text(t)) => d.handle(t.as_str()),
                    Ok(Message::Binary(b)) => d.handle(&String::from_utf8_lossy(&b)),
                    Ok(Message::Close(_)) | Err(_) => break,
                    _ => {}
                }
            }
            d.close();
        });
        Ok(Arc::new(conn))
    }

    /// 管道传输：reader = 子进程 fd 4 的读端，writer = 子进程 fd 3 的写端。阻塞 I/O 放在独立线程。
    pub fn from_pipe(mut reader: Box<dyn Read + Send>, mut writer: Box<dyn Write + Send>) -> Arc<Conn> {
        let (conn, d, mut rx) = Conn::parts("pipe");
        std::thread::Builder::new()
            .name("cdp-pipe-write".into())
            .spawn(move || {
                while let Some(msg) = rx.blocking_recv() {
                    if writer.write_all(msg.as_bytes()).is_err() || writer.write_all(b"\0").is_err() || writer.flush().is_err() {
                        break;
                    }
                }
            })
            .ok();
        std::thread::Builder::new()
            .name("cdp-pipe-read".into())
            .spawn(move || {
                let mut buf: Vec<u8> = Vec::with_capacity(64 * 1024);
                let mut chunk = vec![0u8; 64 * 1024];
                loop {
                    match reader.read(&mut chunk) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            buf.extend_from_slice(&chunk[..n]);
                            while let Some(pos) = buf.iter().position(|b| *b == 0) {
                                let msg: Vec<u8> = buf.drain(..=pos).collect();
                                d.handle(&String::from_utf8_lossy(&msg[..msg.len() - 1]));
                            }
                        }
                    }
                }
                d.close();
            })
            .ok();
        Arc::new(conn)
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.events.subscribe()
    }

    /// 浏览器级命令。
    pub async fn send(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        self.send_to(None, method, params, timeout).await
    }

    /// 发给某个 session（标签页）的命令。
    pub async fn send_to(&self, session: Option<&str>, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        if self.is_closed() {
            return Err("浏览器连接已断开".into());
        }
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        if let Ok(mut m) = self.pending.lock() {
            m.insert(id, tx);
        }
        let mut msg = json!({"id": id, "method": method, "params": params});
        if let Some(s) = session {
            msg["sessionId"] = json!(s);
        }
        if self.tx.send(msg.to_string()).is_err() {
            self.pending.lock().ok().map(|mut m| m.remove(&id));
            return Err("浏览器连接已断开".into());
        }
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(r)) => r,
            Ok(Err(_)) => Err("浏览器连接已断开".into()),
            Err(_) => {
                self.pending.lock().ok().map(|mut m| m.remove(&id));
                Err(format!("CDP 超时：{method}"))
            }
        }
    }
}
