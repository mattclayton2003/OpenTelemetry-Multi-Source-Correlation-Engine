use bank_loadgen::{profile::Profile, runner::run_stage, stats::Stats};
use clap::Parser;

#[derive(Parser)]
struct Cli {
    #[arg(long)]
    profile: std::path::PathBuf,
    #[arg(long, default_value = "/tmp/loadgen-stats.csv")]
    stats_out: std::path::PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _otel = bank_common::otel::init("bank-loadgen")?;
    let cli = Cli::parse();
    let profile: Profile = serde_yaml::from_str(&std::fs::read_to_string(&cli.profile)?)?;
    let stats = Stats::default();

    {
        let stats = stats.clone();
        let out = cli.stats_out.clone();
        tokio::spawn(async move {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&out)
                .unwrap();
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                let _ = f.write_all(stats.snapshot_line().as_bytes());
            }
        });
    }
    let mut handles = vec![];
    for stage in profile.stages {
        handles.push(tokio::spawn(run_stage(stage, stats.clone())));
    }
    for h in handles {
        let _ = h.await;
    }
    Ok(())
}
