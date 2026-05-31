use std::process::Command;

#[test]
fn render_minimal_incident() {
    let exe = env!("CARGO_BIN_EXE_corr");
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/../correlation-core/tests/fixtures/incident_minimal.json");
    let out = Command::new(exe).args(["render", fixture]).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let s = String::from_utf8(out.stdout).unwrap();
    insta::assert_snapshot!(s);
}
