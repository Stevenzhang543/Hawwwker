//! 最小 CDP 连接：一个读取任务按 id 分发响应、广播事件；写入经由通道串行化。
//! （修复 v3.4：同一连接并发 recv 冲突、页面事件被丢弃。）

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;

pub type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>;

pub struct Conn {
    tx: mpsc::UnboundedSender<Message>,
    pending: Pending,
    events: broadcast::Sender<Value>,
    next: AtomicU64,
    closed: Arc<AtomicBool>,
}

impl Conn {
    pub async fn connect(ws_url: &str) -> Result<Arc<Conn>, String> {
        let (ws, _) = tokio::time::timeout(Duration::from_secs(15), tokio_tungstenite::connect_async(ws_url))
            .await
            .map_err(|_| "连接浏览器调试通道超时".to_string())?
            .map_err(|e| format!("连接浏览器调试通道失败：{e}"))?;
        let (mut sink, mut stream) = ws.split();
        let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let (events, _) = broadcast::channel(256);
        let closed = Arc::new(AtomicBool::new(false));

        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                if sink.send(msg).await.is_err() {
                    break;
                }
            }
            let _ = sink.close().await;
        });

        let (p2, e2, c2) = (pending.clone(), events.clone(), closed.clone());
        tokio::spawn(async move {
            while let Some(msg) = stream.next().await {
                let text = match msg {
                    Ok(Message::Text(t)) => t.to_string(),
                    Ok(Message::Binary(b)) => String::from_utf8_lossy(&b).to_string(),
                    Ok(Message::Close(_)) | Err(_) => break,
                    _ => continue,
                };
                let Ok(v) = serde_json::from_str::<Value>(&text) else { continue };
                if let Some(id) = v.get("id").and_then(|x| x.as_u64()) {
                    let waiter = p2.lock().ok().and_then(|mut m| m.remove(&id));
                    if let Some(w) = waiter {
                        let res = match v.get("error") {
                            Some(err) => Err(err.get("message").and_then(|m| m.as_str()).unwrap_or("CDP 错误").to_string()),
                            None => Ok(v.get("result").cloned().unwrap_or(Value::Null)),
                        };
                        let _ = w.send(res);
                    }
                } else if v.get("method").is_some() {
                    let _ = e2.send(v);
                }
            }
            c2.store(true, Ordering::SeqCst);
            if let Ok(mut m) = p2.lock() {
                for (_, w) in m.drain() {
                    let _ = w.send(Err("浏览器连接已断开".into()));
                }
            }
        });

        Ok(Arc::new(Conn { tx, pending, events, next: AtomicU64::new(1), closed }))
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.events.subscribe()
    }

    pub async fn send(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        if self.is_closed() {
            return Err("浏览器连接已断开".into());
        }
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        if let Ok(mut m) = self.pending.lock() {
            m.insert(id, tx);
        }
        let msg = json!({"id": id, "method": method, "params": params}).to_string();
        if self.tx.send(Message::Text(msg.into())).is_err() {
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

    pub fn close(&self) {
        let _ = self.tx.send(Message::Close(None));
    }
}

impl Drop for Conn {
    fn drop(&mut self) {
        let _ = self.tx.send(Message::Close(None));
    }
}
