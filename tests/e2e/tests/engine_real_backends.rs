#![cfg(feature = "e2e")]
use std::path::PathBuf;
use std::process::Command;

/// Locates the compiled `corr` binary. `CARGO_BIN_EXE_corr` is only injected for
/// the crate that defines the binary, so for this external e2e crate we fall
/// back to the workspace target dir (the binary is built by `cargo test
/// --workspace`).
fn corr_binary() -> PathBuf {
    if let Some(p) = option_env!("CARGO_BIN_EXE_corr") {
        return PathBuf::from(p);
    }
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target")
        .join(profile)
        .join("corr")
}

#[test]
fn corr_trace_against_compose() {
    // Assumes `docker compose --profile research up -d` has been run by harness or CI.
    let trace_id =
        std::env::var("E2E_TRACE_ID").expect("set E2E_TRACE_ID to a trace_id known to be in Tempo");
    let out = Command::new(corr_binary())
        .args(["--json", "trace", &trace_id])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["schema_version"], "1.0.0");
    assert!(!v["spans"].as_array().unwrap().is_empty());
}
