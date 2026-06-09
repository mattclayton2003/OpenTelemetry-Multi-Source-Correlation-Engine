use chrono::{DateTime, Utc};
use correlation_core::anomaly::ewma::Ewma;
use correlation_core::anomaly::zscore::ZScore;
use correlation_core::anomaly::Detector;
use correlation_core::backend::{MetricPoint, SpanStatus};
use correlation_core::config::CorrelationConfig;
use correlation_core::graph::builder::EvidenceGraph;
use correlation_core::graph::edges::{Edge, EdgeKind};
use correlation_core::graph::invariants::{check_no_caused_by_cycles, check_no_dangling};
use correlation_core::graph::nodes::Node;
use correlation_core::ranking::scoring::rank_suspects;
use correlation_core::schema::renderer_md::render_md;
use correlation_core::schema::*;
use proptest::prelude::*;
use rand::SeedableRng;

// ---------- shared helpers ----------

/// Approximate float equality that also treats matching infinities as equal
/// (the z-score detectors emit +inf when the baseline stddev is zero).
fn approx(a: f64, b: f64) -> bool {
    if a.is_infinite() || b.is_infinite() {
        return a == b;
    }
    (a - b).abs() <= 1e-6 * (1.0 + a.abs().max(b.abs()))
}

fn series(values: &[f64]) -> Vec<MetricPoint> {
    values
        .iter()
        .enumerate()
        .map(|(i, &v)| MetricPoint {
            ts: DateTime::from_timestamp(1_700_000_000 + i as i64, 0).unwrap(),
            service: "s".into(),
            value: v,
        })
        .collect()
}

// ============================================================
// Graph (pre-existing)
// ============================================================

proptest! {
    #[test]
    fn graph_strict_insertions_preserve_invariants(seed in 0u64..1000) {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let mut g = EvidenceGraph::new();
        let svcs = ["a","b","c","d"];
        for s in &svcs { g.add_node(Node::service((*s).to_string())); }
        for _ in 0..20 {
            let from = svcs[rand::Rng::gen_range(&mut rng, 0..svcs.len())];
            let to   = svcs[rand::Rng::gen_range(&mut rng, 0..svcs.len())];
            let _ = g.add_edge_strict(Edge {
                from: format!("svc:{from}"), to: format!("svc:{to}"),
                kind: EdgeKind::EmittedBy,
            });
        }
        prop_assert!(check_no_dangling(&g).is_ok());
        prop_assert!(check_no_caused_by_cycles(&g).is_ok());
    }
}

// ============================================================
// Anomaly detectors
// ============================================================

proptest! {
    // z-score = (x - mean) / stddev is invariant under translating every point
    // by the same constant, so detection (which point gets flagged and its
    // z-score) must be identical for `vals` and `vals + shift`.
    #[test]
    fn zscore_detection_is_translation_invariant(
        base in prop::collection::vec(-1000.0f64..1000.0, 4..30),
        obs in -1000.0f64..1000.0,
        shift in -500.0f64..500.0,
    ) {
        let mut vals = base.clone();
        vals.push(obs);
        let shifted: Vec<f64> = vals.iter().map(|v| v + shift).collect();
        let det = ZScore { k: 3.0, min_baseline: 3 };
        let a = det.detect(&series(&vals));
        let b = det.detect(&series(&shifted));
        prop_assert_eq!(a.len(), b.len());
        if let (Some(ha), Some(hb)) = (a.first(), b.first()) {
            prop_assert!(approx(ha.z_score, hb.z_score));
        }
    }

    // Both numerator and denominator scale by c > 0, so the z-score and the
    // flagged outcome are invariant under positive scaling.
    #[test]
    fn zscore_detection_is_positive_scale_invariant(
        base in prop::collection::vec(-1000.0f64..1000.0, 4..30),
        obs in -1000.0f64..1000.0,
        scale in 0.25f64..4.0,
    ) {
        let mut vals = base.clone();
        vals.push(obs);
        let scaled: Vec<f64> = vals.iter().map(|v| v * scale).collect();
        let det = ZScore { k: 3.0, min_baseline: 3 };
        let a = det.detect(&series(&vals));
        let b = det.detect(&series(&scaled));
        prop_assert_eq!(a.len(), b.len());
        if let (Some(ha), Some(hb)) = (a.first(), b.first()) {
            prop_assert!(approx(ha.z_score, hb.z_score));
        }
    }

    // A perfectly flat series has zero deviation, so neither detector may flag.
    #[test]
    fn detectors_never_flag_a_constant_series(c in -1e6f64..1e6, n in 5usize..30) {
        let vals = vec![c; n];
        let s = series(&vals);
        let zscore = ZScore { k: 3.0, min_baseline: 3 };
        let ewma = Ewma { alpha: 0.3, k: 3.0, min_baseline: 3 };
        prop_assert!(zscore.detect(&s).is_empty());
        prop_assert!(ewma.detect(&s).is_empty());
    }

    // detect() is a pure function: same input -> same hits.
    #[test]
    fn ewma_detect_is_deterministic(
        vals in prop::collection::vec(-1000.0f64..1000.0, 5..40),
        alpha in 0.05f64..0.95,
        k in 1.0f64..5.0,
    ) {
        let det = Ewma { alpha, k, min_baseline: 3 };
        let s = series(&vals);
        let a = det.detect(&s);
        let b = det.detect(&s);
        prop_assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b.iter()) {
            prop_assert!(approx(x.z_score, y.z_score));
        }
    }
}

// ============================================================
// Ranking
// ============================================================

fn graph_from(
    n_services: usize,
    spans: &[(usize, i64)],
    anomalies: &[(usize, f64)],
) -> EvidenceGraph {
    let mut g = EvidenceGraph::new();
    let names: Vec<String> = (0..n_services).map(|i| format!("svc{i}")).collect();
    let svc_ids: Vec<String> = names
        .iter()
        .map(|n| g.add_node(Node::service(n.clone())))
        .collect();
    for (i, (svc, dur)) in spans.iter().enumerate() {
        let sp = g.add_node(Node::Span {
            id: format!("sp{i}"),
            service: names[*svc].clone(),
            operation: "op".into(),
            status: SpanStatus::Error,
            start: Utc::now(),
            duration_ms: *dur,
            parent: None,
            status_message: None,
        });
        g.add_edge(Edge {
            from: sp,
            to: svc_ids[*svc].clone(),
            kind: EdgeKind::EmittedBy,
        });
    }
    for (i, (svc, sev)) in anomalies.iter().enumerate() {
        let an = g.add_node(Node::MetricAnomaly {
            id: format!("an{i}"),
            service: names[*svc].clone(),
            metric: "m".into(),
            window_start: Utc::now(),
            window_end: Utc::now(),
            severity: *sev,
            detector: "z".into(),
            baseline_mean: 0.0,
            observed_peak: 0.0,
        });
        g.add_edge(Edge {
            from: an,
            to: svc_ids[*svc].clone(),
            kind: EdgeKind::EmittedBy,
        });
    }
    g
}

fn score_of(ranked: &[correlation_core::ranking::ScoredSuspect], service: &str) -> f64 {
    ranked
        .iter()
        .find(|s| s.service == service)
        .map(|s| s.score)
        .unwrap_or(0.0)
}

proptest! {
    // Output has exactly one entry per service node, all scores are
    // non-negative (every evidence weight and the temporal multiplier are
    // >= 0), and entries are sorted by score descending with a service-name
    // tiebreak.
    #[test]
    fn ranking_is_sorted_one_per_service_and_nonneg(
        n in 1usize..6,
        raw_spans in prop::collection::vec((0usize..6, 0i64..2000), 0..15),
        raw_anoms in prop::collection::vec((0usize..6, 0.0f64..5.0), 0..6),
    ) {
        let spans: Vec<_> = raw_spans.into_iter().map(|(s, d)| (s % n, d)).collect();
        let anoms: Vec<_> = raw_anoms.into_iter().map(|(s, sev)| (s % n, sev)).collect();
        let g = graph_from(n, &spans, &anoms);
        let out = rank_suspects(&g, &CorrelationConfig::default(), None);

        prop_assert_eq!(out.len(), n);
        let mut seen = std::collections::HashSet::new();
        for s in &out {
            prop_assert!(seen.insert(s.service.clone()), "duplicate service in ranking");
            prop_assert!(s.score >= 0.0, "negative score: {}", s.score);
        }
        for w in out.windows(2) {
            prop_assert!(w[0].score >= w[1].score, "not sorted descending");
            if w[0].score == w[1].score {
                prop_assert!(w[0].service <= w[1].service, "tie not broken by service name");
            }
        }
    }

    // Ranking is deterministic despite the internal HashMap accumulation: the
    // final ordering uses a total order (score desc, then service name).
    #[test]
    fn ranking_is_deterministic(
        n in 1usize..6,
        raw_spans in prop::collection::vec((0usize..6, 0i64..2000), 0..15),
    ) {
        let spans: Vec<_> = raw_spans.into_iter().map(|(s, d)| (s % n, d)).collect();
        let g = graph_from(n, &spans, &[]);
        let cfg = CorrelationConfig::default();
        let a = rank_suspects(&g, &cfg, None);
        let b = rank_suspects(&g, &cfg, None);
        prop_assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b.iter()) {
            prop_assert_eq!(&x.service, &y.service);
            prop_assert!(approx(x.score, y.score));
        }
    }

    // Adding one more error span to a service never lowers that service's
    // score (evidence is purely additive; propagation only adds).
    #[test]
    fn adding_error_span_never_lowers_score(
        n in 1usize..6,
        raw_spans in prop::collection::vec((0usize..6, 0i64..2000), 0..12),
    ) {
        let spans: Vec<_> = raw_spans.into_iter().map(|(s, d)| (s % n, d)).collect();
        let cfg = CorrelationConfig::default();
        let before = score_of(&rank_suspects(&graph_from(n, &spans, &[]), &cfg, None), "svc0");
        let mut spans2 = spans.clone();
        spans2.push((0, 100));
        let after = score_of(&rank_suspects(&graph_from(n, &spans2, &[]), &cfg, None), "svc0");
        prop_assert!(after >= before - 1e-9, "score dropped: {before} -> {after}");
    }
}

// ============================================================
// Schema (serde round-trip + markdown renderer)
// ============================================================

fn ascii_word() -> impl Strategy<Value = String> {
    prop::collection::vec(prop::char::range('a', 'z'), 1..10)
        .prop_map(|cs| cs.into_iter().collect())
}

/// A string that may contain multi-byte UTF-8 chars and JSON-special chars,
/// to exercise serde escaping and the char-boundary slice in render_md.
fn mixed_text() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            Just('a'),
            Just('Z'),
            Just('0'),
            Just(':'),
            Just(' '),
            Just('"'),
            Just('\\'),
            Just('é'),
            Just('中'),
        ],
        0..20,
    )
    .prop_map(|cs| cs.into_iter().collect())
}

fn arb_dt() -> impl Strategy<Value = DateTime<Utc>> {
    (0i64..4_000_000_000i64).prop_map(|s| DateTime::from_timestamp(s, 0).unwrap())
}

fn arb_window() -> impl Strategy<Value = Window> {
    (arb_dt(), arb_dt(), any::<bool>()).prop_map(|(start, end, expanded)| Window {
        start,
        end,
        expanded,
    })
}

fn arb_contributor() -> impl Strategy<Value = Contributor> {
    (ascii_word(), ascii_word(), -1e9f64..1e9f64).prop_map(|(kind, r, weight)| Contributor {
        kind,
        r#ref: r,
        weight,
    })
}

fn arb_suspect() -> impl Strategy<Value = Suspect> {
    (
        any::<u8>(),
        ascii_word(),
        -1e9f64..1e9f64,
        -1e9f64..1e9f64,
        -1e9f64..1e9f64,
        -1e9f64..1e9f64,
        -1e9f64..1e9f64,
        -1e9f64..1e9f64,
        prop::collection::vec(arb_contributor(), 0..4),
    )
        .prop_map(
            |(rank, service, score, de, da, pw, lat, tm, contributors)| Suspect {
                rank: rank as usize,
                service,
                score,
                evidence_breakdown: EvidenceBreakdown {
                    direct_error_weight: de,
                    direct_anomaly_weight: da,
                    propagated_weight: pw,
                    direct_latency_weight: lat,
                    temporal_tightness_multiplier: tm,
                    contributors,
                },
            },
        )
}

fn arb_service_summary() -> impl Strategy<Value = ServiceSummary> {
    (
        ascii_word(),
        0usize..1000,
        0usize..1000,
        0usize..1000,
        0usize..1000,
    )
        .prop_map(|(name, sc, esc, lc, elc)| ServiceSummary {
            name,
            span_count: sc,
            error_span_count: esc,
            log_count: lc,
            error_log_count: elc,
        })
}

fn arb_trigger() -> impl Strategy<Value = Trigger> {
    prop_oneof![
        ascii_word().prop_map(|t| Trigger::Trace {
            trace: TraceTrigger { trace_id: t }
        }),
        (
            arb_window(),
            -1e9f64..1e9f64,
            -1e9f64..1e9f64,
            -1e9f64..1e9f64,
            -1e9f64..1e9f64
        )
            .prop_map(|(w, ov, bm, bsd, z)| Trigger::Anomaly {
                anomaly: AnomalyTrigger {
                    metric: "m".into(),
                    service: "s".into(),
                    window: w,
                    observed_value: ov,
                    baseline_mean: bm,
                    baseline_stddev: bsd,
                    z_score: z,
                    detector: "z".into(),
                }
            }),
    ]
}

#[allow(clippy::type_complexity)]
fn arb_collections() -> impl Strategy<Value = (Vec<ServiceSummary>, Vec<Suspect>, Vec<String>)> {
    (
        prop::collection::vec(arb_service_summary(), 0..4),
        prop::collection::vec(arb_suspect(), 0..5),
        prop::collection::vec(mixed_text(), 0..4),
    )
}

fn arb_incident() -> impl Strategy<Value = IncidentContext> {
    (
        ascii_word(),  // incident_id
        arb_dt(),      // produced_at
        ascii_word(),  // engine_version
        mixed_text(),  // config_hash (may be multi-byte)
        0u64..100_000, // elapsed_ms
        arb_trigger(),
        arb_window(),
        arb_collections(),
    )
        .prop_map(
            |(id, produced_at, ev, ch, elapsed, trigger, window, (services, suspects, notes))| {
                IncidentContext {
                    schema_version: SCHEMA_VERSION.into(),
                    incident_id: id,
                    produced_at,
                    engine_version: ev,
                    config_hash: ch,
                    elapsed_ms: elapsed,
                    trigger,
                    window,
                    services,
                    suspects,
                    spans: vec![],
                    span_tree: vec![],
                    log_batches: vec![],
                    metric_anomalies: vec![],
                    timeline: vec![],
                    notes,
                }
            },
        )
}

proptest! {
    // The canonical serialized form is a fixpoint: once an incident has been
    // serialized and re-parsed, serializing again is byte-identical.
    //
    // Note we deliberately do NOT assert s1 == s2 on the *first* round-trip:
    // serde_json's default f64 parser is not lossless (it can shift a
    // high-precision float by a ULP), so the in-memory floats and their
    // re-parsed form can differ in the last digit. The reproduce canary
    // tolerates this (epsilon comparison); here we assert the weaker-but-true
    // invariant that the stored/canonical form round-trips exactly, plus that
    // all structural fields survive.
    #[test]
    fn incident_json_canonical_form_is_stable(ic in arb_incident()) {
        let s1 = serde_json::to_string(&ic).unwrap();
        let back: IncidentContext = serde_json::from_str(&s1).unwrap();
        let s2 = serde_json::to_string(&back).unwrap();
        let back2: IncidentContext = serde_json::from_str(&s2).unwrap();
        let s3 = serde_json::to_string(&back2).unwrap();
        prop_assert_eq!(&s2, &s3, "canonical JSON form is not a fixpoint");
        prop_assert_eq!(back.suspects.len(), ic.suspects.len());
        prop_assert_eq!(back.services.len(), ic.services.len());
        prop_assert_eq!(back.notes.len(), ic.notes.len());
        prop_assert_eq!(&back.schema_version, &ic.schema_version);
    }

    // The markdown renderer never panics (incl. multi-byte config_hash) and
    // lists every suspect's service name under the suspects section.
    #[test]
    fn render_md_never_panics_and_lists_suspects(ic in arb_incident()) {
        let md = render_md(&ic);
        prop_assert!(md.contains("## Top suspects"));
        for sus in &ic.suspects {
            prop_assert!(md.contains(&sus.service), "missing suspect {}", sus.service);
        }
    }
}
