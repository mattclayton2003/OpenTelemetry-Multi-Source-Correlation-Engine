use experiment_runner::recovery::{Signal, SignalStateMachine};

#[test]
fn recovery_ts_is_last_of_three_signals() {
    let mut sm = SignalStateMachine::new(std::time::Duration::from_secs(5));
    let t0 = 100_000_000_000_i64;
    sm.observe(Signal::Health,         t0,                  true);
    sm.observe(Signal::LoadGen5xx,     t0 + 1_000_000_000,  true);
    sm.observe(Signal::PromErrorRate,  t0 + 2_000_000_000,  true);
    let recovery = sm.recovery_ts_if_held(t0 + 7_000_000_000);
    assert_eq!(recovery, Some(t0 + 2_000_000_000));
}

#[test]
fn flapping_signal_resets() {
    let mut sm = SignalStateMachine::new(std::time::Duration::from_secs(5));
    let t = 0_i64;
    sm.observe(Signal::Health, t, true);
    sm.observe(Signal::Health, t + 500_000_000, false);
    sm.observe(Signal::Health, t + 1_000_000_000, true);
    sm.observe(Signal::LoadGen5xx, t + 1_500_000_000, true);
    sm.observe(Signal::PromErrorRate, t + 2_000_000_000, true);
    assert_eq!(sm.recovery_ts_if_held(t + 7_000_000_000), Some(t + 2_000_000_000));
}
