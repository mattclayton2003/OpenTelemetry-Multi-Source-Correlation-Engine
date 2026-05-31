pub mod backend;
pub mod config;
pub mod time;
pub mod graph;
pub mod anomaly;
pub mod ranking;
pub mod schema;
pub mod engine;
pub mod backend_multi;

pub use backend::{TelemetryBackend, BackendError};
pub use config::CorrelationConfig;
pub use engine::Engine;
pub use backend_multi::MultiBackend;
pub use schema::IncidentContext;

#[cfg(any(test, feature = "test-helpers"))]
pub mod backend_mock;
