use crate::backend::*;
use async_trait::async_trait;
use std::sync::Arc;

pub struct MultiBackend {
    pub traces:  Arc<dyn TelemetryBackend>,
    pub logs:    Arc<dyn TelemetryBackend>,
    pub metrics: Arc<dyn TelemetryBackend>,
}

#[async_trait]
impl TelemetryBackend for MultiBackend {
    async fn fetch_trace(&self, id: TraceId) -> Result<Vec<Span>, BackendError> { self.traces.fetch_trace(id).await }
    async fn fetch_logs(&self, q: LogQuery) -> Result<Vec<LogRecord>, BackendError> { self.logs.fetch_logs(q).await }
    async fn fetch_metric_series(&self, q: MetricQuery) -> Result<Vec<TimeSeries>, BackendError> { self.metrics.fetch_metric_series(q).await }
    async fn query_metric_window(&self, q: AnomalyWindowQuery) -> Result<Vec<MetricPoint>, BackendError> { self.metrics.query_metric_window(q).await }
}
