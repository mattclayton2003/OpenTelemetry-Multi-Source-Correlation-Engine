#![cfg(feature = "e2e")]
use std::process::Command;

#[test]
fn corr_trace_against_compose() {
    // Assumes `docker compose --profile research up -d` has been run by harness or CI.
    let trace_id = std::env::var("E2E_TRACE_ID")
        .expect("set E2E_TRACE_ID to a trace_id known to be in Tempo");
    let exe = env!("CARGO_BIN_EXE_corr");
    let out = Command::new(exe).args(["--json","trace",&trace_id]).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["schema_version"], "1.0.0");
    assert!(v["spans"].as_array().unwrap().len() > 0);
}
