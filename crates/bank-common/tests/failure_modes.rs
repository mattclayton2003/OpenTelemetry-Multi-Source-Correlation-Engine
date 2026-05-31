use bank_common::failure_modes::FailureModes;

#[test]
fn injects_latency_when_env_set() {
    std::env::set_var("AUTH_INJECT_LATENCY_MS", "50");
    let fm = FailureModes::from_env("AUTH");
    assert_eq!(fm.latency_ms(), Some(50));
    std::env::remove_var("AUTH_INJECT_LATENCY_MS");
}

#[test]
fn error_rate_returns_some_only_when_within_rate() {
    std::env::set_var("AUTH_INJECT_ERROR_RATE", "1.0"); // always
    let fm = FailureModes::from_env("AUTH");
    assert!(fm.should_inject_error());
    std::env::remove_var("AUTH_INJECT_ERROR_RATE");
}
