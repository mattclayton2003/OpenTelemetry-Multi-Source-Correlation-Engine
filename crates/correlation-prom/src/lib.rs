use correlation_core::backend::*;
use async_trait::async_trait;

pub struct PromClient { pub base_url: String, pub http: reqwest::Client, pub retry: RetryPolicy }
impl PromClient { pub fn new(base_url: String) -> Self { Self { base_url, http: reqwest::Client::new(), retry: RetryPolicy::default() } } }

#[async_trait]
impl TelemetryBackend for PromClient {
    async fn fetch_trace(&self, _id: TraceId) -> Result<Vec<Span>, BackendError> { Ok(vec![]) }
    async fn fetch_logs(&self, _q: LogQuery) -> Result<Vec<LogRecord>, BackendError> { Ok(vec![]) }
    async fn fetch_metric_series(&self, q: MetricQuery) -> Result<Vec<TimeSeries>, BackendError> {
        let url = format!("{}/api/v1/query_range", self.base_url);
        let promql = format!("{}{{service=\"{}\"}}", q.metric, q.service);
        let s = q.start.timestamp().to_string();
        let e = q.end.timestamp().to_string();
        let v: serde_json::Value = self.retry.run(|| {
            let url=url.clone(); let http=self.http.clone();
            let promql=promql.clone(); let s=s.clone(); let e=e.clone();
            async move {
                let r = http.get(&url)
                    .query(&[("query", promql.as_str()),("start", s.as_str()),("end", e.as_str()),("step","5")])
                    .send().await?;
                if !r.status().is_success() { return Err(anyhow::anyhow!("status {}", r.status())); }
                Ok(r.json::<serde_json::Value>().await?)
            }
        }).await.map_err(|_| BackendError::Unreachable)?;
        parse_prom_range(v, &q.service, &q.metric)
    }
    async fn query_metric_window(&self, q: AnomalyWindowQuery) -> Result<Vec<MetricPoint>, BackendError> {
        let s = q.start.timestamp().to_string();
        let e = q.end.timestamp().to_string();
        let url = format!("{}/api/v1/query_range", self.base_url);
        let v: serde_json::Value = self.retry.run(|| {
            let url=url.clone(); let http=self.http.clone();
            let metric=q.metric.clone(); let s=s.clone(); let e=e.clone();
            async move {
                let r = http.get(&url)
                    .query(&[("query", metric.as_str()),("start", s.as_str()),("end", e.as_str()),("step","5")])
                    .send().await?;
                if !r.status().is_success() { return Err(anyhow::anyhow!("status {}", r.status())); }
                Ok(r.json::<serde_json::Value>().await?)
            }
        }).await.map_err(|_| BackendError::Unreachable)?;
        parse_prom_points(v)
    }
}

fn parse_prom_range(v: serde_json::Value, service: &str, metric: &str) -> Result<Vec<TimeSeries>, BackendError> {
    use chrono::{Utc, TimeZone};
    let result = v["data"]["result"].as_array().ok_or(BackendError::MalformedResponse)?;
    let mut out = vec![];
    for series in result {
        let mut pts = vec![];
        for v in series["values"].as_array().unwrap_or(&vec![]) {
            let arr = v.as_array().ok_or(BackendError::MalformedResponse)?;
            let ts = arr[0].as_f64().unwrap_or(0.0) as i64;
            let val: f64 = arr[1].as_str().unwrap_or("0").parse().unwrap_or(0.0);
            pts.push((Utc.timestamp_opt(ts, 0).unwrap(), val));
        }
        out.push(TimeSeries { service: service.into(), metric: metric.into(), points: pts });
    }
    Ok(out)
}

fn parse_prom_points(v: serde_json::Value) -> Result<Vec<MetricPoint>, BackendError> {
    use chrono::{Utc, TimeZone};
    let result = v["data"]["result"].as_array().ok_or(BackendError::MalformedResponse)?;
    let mut out = vec![];
    for series in result {
        let svc = series["metric"]["service"].as_str().unwrap_or("unknown").to_string();
        for v in series["values"].as_array().unwrap_or(&vec![]) {
            let arr = v.as_array().ok_or(BackendError::MalformedResponse)?;
            let ts = arr[0].as_f64().unwrap_or(0.0) as i64;
            let val: f64 = arr[1].as_str().unwrap_or("0").parse().unwrap_or(0.0);
            out.push(MetricPoint { ts: Utc.timestamp_opt(ts, 0).unwrap(), service: svc.clone(), value: val });
        }
    }
    Ok(out)
}
