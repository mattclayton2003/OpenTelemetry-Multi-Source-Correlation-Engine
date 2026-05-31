use crate::backend::{TelemetryBackend, TraceId, BackendError, LogQuery, SpanStatus};
use crate::config::CorrelationConfig;
use crate::graph::builder::build_from;
use crate::ranking::scoring::rank_suspects;
use crate::schema::*;
use crate::time::Clock;
use chrono::Duration;
use std::sync::Arc;

pub struct Engine {
    pub backend: Arc<dyn TelemetryBackend>,
    pub cfg: CorrelationConfig,
    pub clock: Arc<dyn Clock>,
}

impl Engine {
    pub fn new(backend: Arc<dyn TelemetryBackend>, cfg: CorrelationConfig, clock: Arc<dyn Clock>) -> Self {
        Self { backend, cfg, clock }
    }

    pub async fn correlate_trace(&self, trace_id: TraceId) -> Result<IncidentContext, BackendError> {
        let t0 = std::time::Instant::now();
        let spans_result = self.backend.fetch_trace(trace_id.clone()).await;
        let spans = match spans_result {
            Ok(s) => s,
            Err(BackendError::Empty) => {
                return Ok(self.empty_incident(
                    Trigger::Trace { trace: TraceTrigger { trace_id } },
                    vec!["trace not found in backend".into()],
                    t0,
                ));
            }
            Err(e) => return Err(e),
        };
        if spans.is_empty() {
            return Ok(self.empty_incident(
                Trigger::Trace { trace: TraceTrigger { trace_id } },
                vec!["trace not found".into()], t0,
            ));
        }
        let mut services: Vec<String> = spans.iter().map(|s| s.service.clone()).collect();
        services.sort(); services.dedup();
        let t_min = spans.iter().map(|s| s.start).min().unwrap();
        let t_max = spans.iter().map(|s| s.start + Duration::milliseconds(s.duration_ms)).max().unwrap();
        let exp = Duration::seconds(self.cfg.window_expansion_sec);
        let start = t_min - exp; let end = t_max + exp;
        let logs = self.backend.fetch_logs(LogQuery {
            services: services.clone(), start, end, level_at_least: None,
        }).await.unwrap_or_default();
        let g = build_from(&spans, &logs, &[], &self.cfg);
        let suspects = rank_suspects(&g, &self.cfg, None);
        Ok(IncidentContext {
            schema_version: SCHEMA_VERSION.into(),
            incident_id: uuid::Uuid::now_v7().to_string(),
            produced_at: self.clock.now(),
            engine_version: env!("CARGO_PKG_VERSION").into(),
            config_hash: self.cfg.hash(),
            elapsed_ms: t0.elapsed().as_millis() as u64,
            trigger: Trigger::Trace { trace: TraceTrigger { trace_id } },
            window: Window { start, end, expanded: true },
            services: services.iter().map(|name| ServiceSummary {
                name: name.clone(),
                span_count: spans.iter().filter(|s| s.service == *name).count(),
                error_span_count: spans.iter().filter(|s| s.service == *name && s.status == SpanStatus::Error).count(),
                log_count: logs.iter().filter(|l| l.service == *name).count(),
                error_log_count: logs.iter().filter(|l| l.service == *name && l.level == "ERROR").count(),
            }).collect(),
            suspects: suspects.into_iter().enumerate().map(|(i, s)| Suspect {
                rank: i + 1, service: s.service, score: s.score,
                evidence_breakdown: EvidenceBreakdown {
                    direct_error_weight: s.direct_error,
                    direct_anomaly_weight: s.direct_anomaly,
                    propagated_weight: s.propagated,
                    temporal_tightness_multiplier: s.temporal_mult,
                    contributors: s.contributors.into_iter().map(|(kind, r, w)|
                        Contributor { kind, r#ref: r, weight: w }).collect(),
                },
            }).collect(),
            spans: spans.iter().map(|s| SpanRef {
                id: s.span_id.clone(), trace_id: s.trace_id.clone(),
                parent_id: s.parent_id.clone(), service: s.service.clone(),
                operation: s.operation.clone(), start: s.start, duration_ms: s.duration_ms,
                status: match s.status { SpanStatus::Ok => "OK".into(), SpanStatus::Error => "ERROR".into() },
                status_message: s.status_message.clone(),
                attributes: s.attributes.clone().into_iter().collect(),
            }).collect(),
            span_tree: build_tree(&spans),
            log_batches: vec![],
            metric_anomalies: vec![],
            timeline: vec![],
            notes: vec![],
        })
    }

    fn empty_incident(&self, trigger: Trigger, notes: Vec<String>, t0: std::time::Instant) -> IncidentContext {
        let now = self.clock.now();
        IncidentContext {
            schema_version: SCHEMA_VERSION.into(), incident_id: uuid::Uuid::now_v7().to_string(),
            produced_at: now, engine_version: env!("CARGO_PKG_VERSION").into(),
            config_hash: self.cfg.hash(), elapsed_ms: t0.elapsed().as_millis() as u64,
            trigger, window: Window { start: now, end: now, expanded: false },
            services: vec![], suspects: vec![], spans: vec![], span_tree: vec![],
            log_batches: vec![], metric_anomalies: vec![], timeline: vec![],
            notes,
        }
    }
}

fn build_tree(spans: &[crate::backend::Span]) -> Vec<TreeNode> {
    use std::collections::HashMap;
    let mut children: HashMap<Option<String>, Vec<String>> = HashMap::new();
    for s in spans {
        children.entry(s.parent_id.clone()).or_default().push(s.span_id.clone());
    }
    fn build(id: &str, children: &HashMap<Option<String>, Vec<String>>) -> TreeNode {
        let kids = children.get(&Some(id.to_string())).cloned().unwrap_or_default();
        TreeNode { span_id: id.into(), children: kids.iter().map(|c| build(c, children)).collect() }
    }
    children.get(&None).cloned().unwrap_or_default().iter().map(|root| build(root, &children)).collect()
}
