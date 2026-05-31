pub mod backend;
pub mod config;
pub mod time;
pub mod graph;
pub mod anomaly;
pub mod ranking;
pub mod schema;
pub mod engine;

pub use backend::{TelemetryBackend, BackendError};
pub use config::CorrelationConfig;
pub use engine::Engine;
pub use schema::IncidentContext;

#[cfg(any(test, feature = "test-helpers"))]
pub mod backend_mock;
