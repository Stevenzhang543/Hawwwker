//! 搜索：规则预解析、卡片解析、过滤聚类、编排管线与运行注册表（事件回放 + 取消）。

pub mod filter;
pub mod parse;
pub mod pipeline;
pub mod rules;

use crate::models::{RunEvent, RunEventBody, RunEventPayload};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

pub struct RunHandle {
    pub id: String,
    pub cancel: CancellationToken,
    events: Mutex<Vec<RunEvent>>,
    seq: AtomicU64,
    finished: AtomicBool,
    finished_at: Mutex<Option<Instant>>,
}

impl RunHandle {
    pub fn emit(&self, app: Option<&AppHandle>, body: RunEventBody) {
        let terminal = body.is_terminal();
        if self.finished.load(Ordering::SeqCst) {
            return; // 终态之后不再发事件
        }
        let ev = RunEvent { body, seq: self.seq.fetch_add(1, Ordering::SeqCst) + 1, t: chrono::Utc::now().timestamp_millis() };
        if let Ok(mut e) = self.events.lock() {
            e.push(ev.clone());
        }
        if terminal {
            self.finished.store(true, Ordering::SeqCst);
            if let Ok(mut f) = self.finished_at.lock() {
                *f = Some(Instant::now());
            }
        }
        if let Some(app) = app {
            let _ = app.emit("run://event", RunEventPayload { run_id: self.id.clone(), event: ev });
        }
    }

    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::SeqCst)
    }

    pub fn since(&self, seq: u64) -> Vec<RunEvent> {
        self.events.lock().map(|e| e.iter().filter(|x| x.seq > seq).cloned().collect()).unwrap_or_default()
    }
}

#[derive(Default)]
pub struct RunRegistry {
    runs: Mutex<HashMap<String, Arc<RunHandle>>>,
}

impl RunRegistry {
    pub fn create(&self, id: &str) -> Arc<RunHandle> {
        self.gc();
        let h = Arc::new(RunHandle {
            id: id.to_string(),
            cancel: CancellationToken::new(),
            events: Mutex::new(vec![]),
            seq: AtomicU64::new(0),
            finished: AtomicBool::new(false),
            finished_at: Mutex::new(None),
        });
        if let Ok(mut m) = self.runs.lock() {
            m.insert(id.to_string(), h.clone());
        }
        h
    }

    pub fn get(&self, id: &str) -> Option<Arc<RunHandle>> {
        self.runs.lock().ok()?.get(id).cloned()
    }

    pub fn running(&self) -> Vec<String> {
        self.runs.lock().map(|m| m.values().filter(|h| !h.is_finished()).map(|h| h.id.clone()).collect()).unwrap_or_default()
    }

    pub fn cancel_all(&self) {
        if let Ok(m) = self.runs.lock() {
            for h in m.values() {
                h.cancel.cancel();
            }
        }
    }

    /// 结束超过 1 小时的运行从内存移除（结果仍在数据库中）。
    fn gc(&self) {
        if let Ok(mut m) = self.runs.lock() {
            m.retain(|_, h| h.finished_at.lock().ok().and_then(|f| *f).is_none_or(|t| t.elapsed().as_secs() < 3600));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_are_sequenced_and_stop_after_terminal() {
        let reg = RunRegistry::default();
        let h = reg.create("r1");
        h.emit(None, RunEventBody::Log { level: "info".into(), message: "a".into() });
        h.emit(None, RunEventBody::Cancelled);
        h.emit(None, RunEventBody::Log { level: "info".into(), message: "late".into() });
        let ev = h.since(0);
        assert_eq!(ev.len(), 2);
        assert_eq!(ev[0].seq, 1);
        assert_eq!(ev[1].seq, 2);
        assert_eq!(h.since(1).len(), 1);
        assert!(reg.running().is_empty());
    }
}
