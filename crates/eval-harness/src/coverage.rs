use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct CoverageTargets {
    #[serde(flatten)] pub classes: HashMap<String, ClassEntry>,
}
#[derive(Debug, Deserialize)]
pub struct ClassEntry {
    pub metrics: Vec<String>,
}

impl CoverageTargets {
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        Ok(toml::from_str(&std::fs::read_to_string(path)?)?)
    }
    pub fn expected_for(&self, class: &str) -> Vec<String> {
        self.classes.get(class).map(|e| e.metrics.clone()).unwrap_or_default()
    }
}

#[derive(Debug, Deserialize)]
pub struct AnomalyInvocation {
    #[serde(flatten)] pub classes: HashMap<String, InvocationEntry>,
}
#[derive(Debug, Deserialize)]
pub struct InvocationEntry {
    pub metric: String, pub service: String,
    pub window_pre_sec: i64, pub window_post_sec: i64,
}
impl AnomalyInvocation {
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        Ok(toml::from_str(&std::fs::read_to_string(path)?)?)
    }
}
