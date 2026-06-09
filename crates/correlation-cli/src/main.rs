use clap::{Parser, Subcommand};
use correlation_core::time::WallClock;
use correlation_core::{CorrelationConfig, Engine, MultiBackend};
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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if let Cmd::Render { path } = &cli.cmd {
        let ic: correlation_core::IncidentContext =
            serde_json::from_reader(std::fs::File::open(path)?)?;
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
        Cmd::Render { .. } => unreachable!(),
    };
    if cli.json {
        println!("{}", serde_json::to_string_pretty(&ic)?);
    } else {
        println!("{}", correlation_core::schema::renderer_md::render_md(&ic));
    }
    Ok(())
}
