use std::collections::BTreeMap;
use std::time::Duration;

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub enum Signal { Health, LoadGen5xx, PromErrorRate }

pub struct SignalStateMachine {
    grace: Duration,
    cleared_at: BTreeMap<Signal, i64>,
}

impl SignalStateMachine {
    pub fn new(grace: Duration) -> Self { Self { grace, cleared_at: BTreeMap::new() } }
    pub fn observe(&mut self, sig: Signal, ts_ns: i64, ok: bool) {
        if ok { self.cleared_at.entry(sig).or_insert(ts_ns); } else { self.cleared_at.remove(&sig); }
    }
    pub fn recovery_ts_if_held(&self, now_ns: i64) -> Option<i64> {
        if self.cleared_at.len() < 3 { return None; }
        let last = *self.cleared_at.values().max().unwrap();
        if now_ns - last >= self.grace.as_nanos() as i64 { Some(last) } else { None }
    }
}
