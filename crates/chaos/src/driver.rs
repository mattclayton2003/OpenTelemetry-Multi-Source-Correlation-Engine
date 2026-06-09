use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FaultSpec {
    Toxiproxy {
        proxy: String,
        toxic: crate::toxiproxy::Toxic,
    },
    PumbaKill {
        container: String,
    },
    PumbaPause {
        container: String,
        duration_sec: u32,
    },
    PumbaStress {
        container: String,
        cpus: u32,
        duration_sec: u32,
    },
}

#[derive(Debug, Clone)]
pub struct FaultHandle {
    pub spec: FaultSpec,
    pub revert_token: String,
}

#[async_trait]
pub trait FaultDriver: Send + Sync {
    async fn apply(&self, spec: &FaultSpec) -> Result<FaultHandle>;
    async fn revert(&self, handle: &FaultHandle) -> Result<()>;
}

pub struct DefaultDriver {
    pub toxi: crate::toxiproxy::ToxiproxyClient,
}

#[async_trait]
impl FaultDriver for DefaultDriver {
    async fn apply(&self, spec: &FaultSpec) -> Result<FaultHandle> {
        match spec {
            FaultSpec::Toxiproxy { proxy, toxic } => {
                let token = self.toxi.add_toxic(proxy, toxic.clone()).await?;
                Ok(FaultHandle {
                    spec: spec.clone(),
                    revert_token: token,
                })
            }
            FaultSpec::PumbaKill { container } => {
                crate::pumba::kill(container).await?;
                Ok(FaultHandle {
                    spec: spec.clone(),
                    revert_token: "".into(),
                })
            }
            FaultSpec::PumbaPause {
                container,
                duration_sec,
            } => {
                crate::pumba::pause(container, *duration_sec).await?;
                Ok(FaultHandle {
                    spec: spec.clone(),
                    revert_token: "".into(),
                })
            }
            FaultSpec::PumbaStress {
                container,
                cpus,
                duration_sec,
            } => {
                crate::pumba::stress(container, *cpus, *duration_sec).await?;
                Ok(FaultHandle {
                    spec: spec.clone(),
                    revert_token: "".into(),
                })
            }
        }
    }
    async fn revert(&self, h: &FaultHandle) -> Result<()> {
        match &h.spec {
            FaultSpec::Toxiproxy { proxy, .. } => {
                self.toxi.remove_toxic(proxy, &h.revert_token).await
            }
            _ => Ok(()),
        }
    }
}
