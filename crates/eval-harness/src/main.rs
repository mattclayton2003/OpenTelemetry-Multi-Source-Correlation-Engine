use clap::Parser;
use eval_harness::{db, report, runner};

#[derive(Parser)]
struct Cli {
    #[arg(long, default_value = "data/labels.db")]
    labels: std::path::PathBuf,
    #[arg(long, default_value = "data/incidents.db")]
    incidents: std::path::PathBuf,
    #[arg(long, default_value = "data/eval_runs.db")]
    eval: std::path::PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(clap::Subcommand)]
enum Cmd {
    Run {
        #[arg(long)]
        suite: String,
        #[arg(long, default_value = "configs/default.toml")]
        config: std::path::PathBuf,
        #[arg(long, default_value = "configs/scoring.toml")]
        scoring: std::path::PathBuf,
        #[arg(long, default_value = "configs/coverage_targets.toml")]
        coverage: std::path::PathBuf,
        #[arg(long, default_value = "configs/anomaly_invocation.toml")]
        invocation: std::path::PathBuf,
        #[arg(long)]
        tag: String,
    },
    Report {
        #[arg(long)]
        tag: String,
    },
    Reproduce {
        eval_run_id: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let labels = db::open(&cli.labels).await?;
    let incidents = db::open(&cli.incidents).await?;
    let eval = db::open(&cli.eval).await?;

    // Run migrations on each DB. labels migrations live in the
    // experiment-runner crate (eval invokes run_file which expects schema).
    sqlx::migrate!("./migrations_eval").run(&eval).await?;
    sqlx::migrate!("./migrations_incidents")
        .run(&incidents)
        .await?;
    sqlx::migrate!("../experiment-runner/migrations")
        .run(&labels)
        .await?;

    match cli.cmd {
        Cmd::Run {
            suite,
            config,
            scoring,
            coverage,
            invocation,
            tag,
        } => {
            runner::run_from_files(runner::EvalRunArgs {
                labels,
                incidents,
                eval,
                suite_glob: suite,
                config_path: config,
                scoring_path: scoring,
                coverage_path: coverage,
                invocation_path: invocation,
                tag,
            })
            .await?;
        }
        Cmd::Report { tag } => report::print_for_tag(&eval, &tag).await?,
        Cmd::Reproduce { eval_run_id } => report::reproduce(&eval, &eval_run_id).await?,
    }
    Ok(())
}
