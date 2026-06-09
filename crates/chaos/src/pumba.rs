use anyhow::Result;
use tokio::process::Command;

pub async fn kill(container: &str) -> Result<()> {
    let st = Command::new("pumba")
        .args(["kill", "--signal", "SIGKILL", container])
        .status()
        .await?;
    anyhow::ensure!(st.success(), "pumba kill failed");
    Ok(())
}

pub async fn pause(container: &str, duration_sec: u32) -> Result<()> {
    let dur = format!("{duration_sec}s");
    let st = Command::new("pumba")
        .args(["pause", "--duration", dur.as_str(), container])
        .status()
        .await?;
    anyhow::ensure!(st.success(), "pumba pause failed");
    Ok(())
}

pub async fn stress(container: &str, cpus: u32, duration_sec: u32) -> Result<()> {
    let dur = format!("{duration_sec}s");
    let cpus_s = format!("{cpus}");
    let stressors = format!("--cpu {cpus_s} --timeout {duration_sec}s");
    let st = Command::new("pumba")
        .args([
            "--log-level",
            "info",
            "stress",
            "--stress-image",
            "alexeiled/stress-ng:latest",
            "--duration",
            dur.as_str(),
            "--stressors",
            &stressors,
            container,
        ])
        .status()
        .await?;
    anyhow::ensure!(st.success(), "pumba stress failed");
    Ok(())
}
