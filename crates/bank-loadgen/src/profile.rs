use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Profile {
    pub stages: Vec<Stage>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Stage {
    pub endpoint: String,
    pub rps: u32,
    pub duration_sec: u32,
    pub start_offset_sec: Option<u32>,
    pub body: Option<serde_json::Value>,
}
