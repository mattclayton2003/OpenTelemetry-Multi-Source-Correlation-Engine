use clap::Parser;

#[derive(Parser)]
struct Cli {
    #[arg(long, default_value = "data/labels.db")]
    db: std::path::PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(clap::Subcommand)]
enum Cmd {
    Run {
        yaml: std::path::PathBuf,
        #[arg(long)]
        dry_run: bool,
    },
    Suite {
        glob: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _otel = bank_common::otel::init("experiment-runner").ok();
    let cli = Cli::parse();
    let pool = experiment_runner::db::open(&cli.db).await?;
    match cli.cmd {
        Cmd::Run { yaml, dry_run } => {
            experiment_runner::runner::run_file(&yaml, &pool, dry_run).await?;
        }
        Cmd::Suite { glob } => {
            for entry in glob::glob(&glob)? {
                experiment_runner::runner::run_file(&entry?, &pool, false).await?;
            }
        }
    }
    Ok(())
}
