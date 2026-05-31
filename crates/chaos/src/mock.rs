use super::driver::*;
use anyhow::Result;
use async_trait::async_trait;
use std::sync::{Arc, Mutex};

#[derive(Default, Clone)]
pub struct MockDriver {
    pub applied: Arc<Mutex<Vec<FaultSpec>>>,
    pub reverted: Arc<Mutex<Vec<String>>>,
    pub fail_revert: bool,
}

#[async_trait]
impl FaultDriver for MockDriver {
    async fn apply(&self, spec: &FaultSpec) -> Result<FaultHandle> {
        self.applied.lock().unwrap().push(spec.clone());
        Ok(FaultHandle { spec: spec.clone(), revert_token: "tok".into() })
    }
    async fn revert(&self, h: &FaultHandle) -> Result<()> {
        if self.fail_revert { anyhow::bail!("mock revert failed"); }
        self.reverted.lock().unwrap().push(h.revert_token.clone());
        Ok(())
    }
}
