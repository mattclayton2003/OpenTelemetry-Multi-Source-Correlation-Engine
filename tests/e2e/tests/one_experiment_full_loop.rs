#![cfg(feature = "e2e")]
use std::process::Command;

#[test]
fn payment_storm_full_loop() {
    // 1. Run runner via compose exec
    let st = Command::new("docker")
        .args(["compose","-f","compose/docker-compose.yaml","exec","-T","experiment-runner",
               "exp","run","/experiments/payment-storm-001.yaml"]).status().unwrap();
    assert!(st.success(), "runner exec failed");

    // 2. Run eval over the one experiment
    let st = Command::new("docker")
        .args(["compose","-f","compose/docker-compose.yaml","exec","-T","eval-harness",
               "eval","run","--suite","/experiments/payment-storm-001.yaml","--tag","e2e"])
        .status().unwrap();
    assert!(st.success(), "eval exec failed");

    // 3. Verify labels.db has the experiment row
    let out = Command::new("sqlite3").args(["data/labels.db",
        "SELECT id, status FROM experiments;"]).output().unwrap();
    let s = String::from_utf8(out.stdout).unwrap();
    assert!(s.contains("payment-storm-001"), "expected experiment row, got: {s}");
}
