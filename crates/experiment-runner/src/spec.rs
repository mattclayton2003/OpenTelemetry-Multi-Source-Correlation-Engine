use serde::{Deserialize, Serialize};

// Note: `Experiment` and `Load` derive only `Deserialize` because
// `bank_loadgen::profile::Stage` is `Deserialize`-only and Rule 1 forbids
// modifying it. The runner only ever deserializes these from YAML and
// serializes the inner `FaultSpec` (which is `Serialize`) for the labels DB.
#[derive(Debug, Clone, Deserialize)]
pub struct Experiment {
    pub id: String,
    pub description: Option<String>,
    pub duration_sec: u32,
    pub warmup_sec: u32,
    pub cooldown_sec: u32,
    pub recovery_grace_sec: u32,
    pub load: Load,
    pub faults: Vec<Fault>,
    pub ground_truth: GroundTruth,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Load { pub generator: String, pub profile: Vec<bank_loadgen::profile::Stage> }

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Fault {
    pub at_sec: u32,
    pub until_sec: u32,
    #[serde(flatten)] pub spec: chaos::driver::FaultSpec,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GroundTruth {
    pub primary_faulted_service: String,
    pub expected_blast_radius: Vec<String>,
    pub expected_clean_services: Vec<String>,
    pub failure_class: String,
}
