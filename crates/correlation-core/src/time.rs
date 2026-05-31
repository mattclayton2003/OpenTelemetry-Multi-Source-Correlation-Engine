use chrono::{DateTime, Utc};

pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

pub struct WallClock;
impl Clock for WallClock { fn now(&self) -> DateTime<Utc> { Utc::now() } }

pub struct TestClock { pub now: DateTime<Utc> }
impl Clock for TestClock { fn now(&self) -> DateTime<Utc> { self.now } }
