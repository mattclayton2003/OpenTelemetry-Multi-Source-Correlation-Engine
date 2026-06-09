use crate::backend::*;
use async_trait::async_trait;
use std::path::PathBuf;

pub struct MockBackend {
    pub trace_by_id: indexmap::IndexMap<TraceId, Vec<Span>>,
    pub all_logs: Vec<LogRecord>,
    pub all_metric_pts: Vec<MetricPoint>,
    pub all_series: Vec<TimeSeries>,
}

impl MockBackend {
    pub fn from_fixture_dir(dir: PathBuf) -> anyhow::Result<Self> {
        let read = |name: &str| -> anyhow::Result<serde_json::Value> {
            let p = dir.join(name);
            Ok(serde_json::from_str(&std::fs::read_to_string(&p)?)?)
        };
        let traces_json = read("tempo.json")?;
        let logs_json = read("loki.json")?;
        let prom_json = read("prom.json")?;

        let mut trace_by_id = indexmap::IndexMap::new();
        for s in traces_json.as_array().cloned().unwrap_or_default() {
            let span: Span = serde_json::from_value(s)?;
            trace_by_id
                .entry(span.trace_id.clone())
                .or_insert_with(Vec::new)
                .push(span);
        }
        let all_logs: Vec<LogRecord> = serde_json::from_value(logs_json["records"].clone())?;
        let all_metric_pts: Vec<MetricPoint> =
            serde_json::from_value(prom_json["points"].clone()).unwrap_or_default();
        let all_series: Vec<TimeSeries> =
            serde_json::from_value(prom_json["series"].clone()).unwrap_or_default();
        Ok(Self {
            trace_by_id,
            all_logs,
            all_metric_pts,
            all_series,
        })
    }
}

#[async_trait]
impl TelemetryBackend for MockBackend {
    async fn fetch_trace(&self, id: TraceId) -> Result<Vec<Span>, BackendError> {
        self.trace_by_id
            .get(&id)
            .cloned()
            .ok_or(BackendError::Empty)
    }
    async fn fetch_logs(&self, q: LogQuery) -> Result<Vec<LogRecord>, BackendError> {
        Ok(self
            .all_logs
            .iter()
            .filter(|l| q.services.contains(&l.service) && l.ts >= q.start && l.ts <= q.end)
            .cloned()
            .collect())
    }
    async fn fetch_metric_series(&self, q: MetricQuery) -> Result<Vec<TimeSeries>, BackendError> {
        Ok(self
            .all_series
            .iter()
            .filter(|s| s.service == q.service && s.metric == q.metric)
            .cloned()
            .collect())
    }
    async fn query_metric_window(
        &self,
        q: AnomalyWindowQuery,
    ) -> Result<Vec<MetricPoint>, BackendError> {
        Ok(self
            .all_metric_pts
            .iter()
            .filter(|p| p.ts >= q.start && p.ts <= q.end)
            .cloned()
            .collect())
    }
}
