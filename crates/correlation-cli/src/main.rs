use clap::{Parser, Subcommand};
use correlation_core::time::WallClock;
use correlation_core::{CorrelationConfig, Engine, IncidentContext, MultiBackend};
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "corr")]
struct Cli {
    #[arg(long, env = "TEMPO_URL", default_value = "http://localhost:3200")]
    tempo: String,
    #[arg(long, env = "LOKI_URL", default_value = "http://localhost:3100")]
    loki: String,
    #[arg(long, env = "PROM_URL", default_value = "http://localhost:9090")]
    prom: String,
    #[arg(long)]
    json: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    Trace {
        trace_id: String,
    },
    Anomaly {
        #[arg(long)]
        metric: String,
        #[arg(long)]
        service: String,
        #[arg(long)]
        start: chrono::DateTime<chrono::Utc>,
        #[arg(long)]
        end: chrono::DateTime<chrono::Utc>,
        #[arg(long)]
        value: f64,
    },
    Render {
        path: std::path::PathBuf,
    },
    /// Produce a plain-English root-cause narrative from an incident, via the
    /// Claude API. Either correlate a live trace (`--trace-id`) or explain a
    /// previously-saved incident document (`--incident <incident.json>`).
    Explain {
        /// Correlate this trace id live, then explain the result.
        #[arg(long, conflicts_with = "incident")]
        trace_id: Option<String>,
        /// Explain a saved IncidentContext JSON instead of correlating live.
        #[arg(long, conflicts_with = "trace_id")]
        incident: Option<std::path::PathBuf>,
        /// Anthropic model id.
        #[arg(long, env = "CORR_EXPLAIN_MODEL", default_value = "claude-opus-4-8")]
        model: String,
        /// Print the grounded prompt that would be sent, without calling the API.
        #[arg(long)]
        dry_run: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Render is fully offline (no telemetry backends needed).
    if let Cmd::Render { path } = &cli.cmd {
        let ic: IncidentContext = serde_json::from_reader(std::fs::File::open(path)?)?;
        println!("{}", correlation_core::schema::renderer_md::render_md(&ic));
        return Ok(());
    }

    let backend = MultiBackend {
        traces: Arc::new(correlation_tempo::TempoClient::new(cli.tempo)),
        logs: Arc::new(correlation_loki::LokiClient::new(cli.loki)),
        metrics: Arc::new(correlation_prom::PromClient::new(cli.prom)),
    };
    let engine = Engine::new(
        Arc::new(backend),
        CorrelationConfig::default(),
        Arc::new(WallClock),
    );

    // Explain may be offline (when given --incident) or live (--trace-id); it
    // builds its own incident and returns early without the JSON/md dump below.
    if let Cmd::Explain {
        trace_id,
        incident,
        model,
        dry_run,
    } = &cli.cmd
    {
        let ic = match (incident, trace_id) {
            (Some(path), _) => serde_json::from_reader(std::fs::File::open(path)?)?,
            (None, Some(tid)) => engine.correlate_trace(tid.clone()).await?,
            (None, None) => {
                anyhow::bail!("corr explain needs --trace-id <id> or --incident <incident.json>")
            }
        };
        return explain(&ic, model, *dry_run).await;
    }

    let ic = match cli.cmd {
        Cmd::Trace { trace_id } => engine.correlate_trace(trace_id).await?,
        Cmd::Anomaly {
            metric,
            service,
            start,
            end,
            value,
        } => {
            engine
                .correlate_anomaly(metric, service, start, end, value)
                .await?
        }
        Cmd::Render { .. } | Cmd::Explain { .. } => unreachable!(),
    };
    if cli.json {
        println!("{}", serde_json::to_string_pretty(&ic)?);
    } else {
        println!("{}", correlation_core::schema::renderer_md::render_md(&ic));
    }
    Ok(())
}

/// Sends the grounded incident document to the Claude API and prints the
/// model's root-cause / blast-radius / remediation narrative.
///
/// The incident JSON is the *only* grounding — the system prompt instructs the
/// model to cite the evidence the engine already gathered and not to invent
/// services, metrics, or causes. With `--dry-run` (or no `ANTHROPIC_API_KEY`)
/// it prints what would be sent rather than calling the API.
async fn explain(ic: &IncidentContext, model: &str, dry_run: bool) -> anyhow::Result<()> {
    let doc = serde_json::to_string_pretty(ic)?;
    let system = "You are an SRE incident assistant. You are given a machine-generated incident \
context document (JSON) from a distributed-tracing correlation engine that has already gathered \
and ranked the evidence across traces, logs, and metrics. Explain the incident in plain English \
for an on-call engineer, in three short sections:\n\
1. Root cause — name the top-ranked suspect service and cite the specific evidence the engine \
used (latency self-time, error spans, metric anomaly, or propagation from a dependency).\n\
2. Blast radius — which services are affected versus clean, and why (distinguish a slow/failing \
worker from a caller merely blocked waiting on it).\n\
3. Recommended next step — one concrete action to confirm or remediate.\n\
Ground every claim in the document. Do not invent services, metrics, error messages, or causes \
that are not present. If the evidence is thin or the incident is degraded (see the notes), say \
so plainly rather than guessing.";
    let user = format!("Incident context document:\n\n```json\n{doc}\n```");

    if dry_run {
        println!("# system\n{system}\n\n# user\n{user}");
        return Ok(());
    }

    let api_key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
        anyhow::anyhow!(
            "ANTHROPIC_API_KEY is not set — export it, or re-run with --dry-run to see the prompt"
        )
    })?;

    // Adaptive thinking: root-cause reasoning over the evidence graph benefits
    // from it, and the thinking content is omitted from the response by default
    // (we only read the final text blocks). Single short call, so no streaming.
    let body = serde_json::json!({
        "model": model,
        "max_tokens": 2048,
        "thinking": { "type": "adaptive" },
        "system": system,
        "messages": [{ "role": "user", "content": user }],
    });

    let resp = reqwest::Client::new()
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await?;

    let status = resp.status();
    let payload: serde_json::Value = resp.json().await?;
    if !status.is_success() {
        anyhow::bail!("Anthropic API error {status}: {payload}");
    }

    // Concatenate the text content blocks (skip any thinking blocks).
    let text: String = payload["content"]
        .as_array()
        .map(|blocks| {
            blocks
                .iter()
                .filter(|b| b["type"] == "text")
                .filter_map(|b| b["text"].as_str())
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default();
    if text.trim().is_empty() {
        anyhow::bail!("Anthropic API returned no text content: {payload}");
    }
    println!("{}", text.trim());
    Ok(())
}
