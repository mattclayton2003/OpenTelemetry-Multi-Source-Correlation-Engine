use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use chrono::Utc;

#[derive(Default)]
pub struct Bucket {
    pub success: AtomicU64,
    pub four_xx: AtomicU64,
    pub five_xx: AtomicU64,
    pub error:   AtomicU64,
    pub p99_ms:  AtomicU64,
}

#[derive(Clone, Default)]
pub struct Stats {
    pub current: Arc<Bucket>,
}

impl Stats {
    pub fn snapshot_line(&self) -> String {
        let ts = Utc::now();
        format!(
            "{},{},{},{},{},{}\n",
            ts.to_rfc3339(),
            self.current.success.swap(0, Ordering::SeqCst),
            self.current.four_xx.swap(0, Ordering::SeqCst),
            self.current.five_xx.swap(0, Ordering::SeqCst),
            self.current.error.swap(0, Ordering::SeqCst),
            self.current.p99_ms.swap(0, Ordering::SeqCst),
        )
    }
}
