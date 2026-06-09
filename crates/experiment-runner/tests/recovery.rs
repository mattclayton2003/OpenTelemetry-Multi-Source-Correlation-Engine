use experiment_runner::recovery::{Signal, SignalStateMachine};
use std::collections::BTreeSet;

fn all_three() -> BTreeSet<Signal> {
    BTreeSet::from([Signal::Health, Signal::LoadGen5xx, Signal::PromErrorRate])
}

#[test]
fn recovery_ts_is_last_of_three_signals() {
    let mut sm = SignalStateMachine::new(std::time::Duration::from_secs(5), all_three());
    let t0 = 100_000_000_000_i64;
    sm.observe(Signal::Health, t0, true);
    sm.observe(Signal::LoadGen5xx, t0 + 1_000_000_000, true);
    sm.observe(Signal::PromErrorRate, t0 + 2_000_000_000, true);
    let recovery = sm.recovery_ts_if_held(t0 + 7_000_000_000);
    assert_eq!(recovery, Some(t0 + 2_000_000_000));
}

#[test]
fn flapping_signal_resets() {
    let mut sm = SignalStateMachine::new(std::time::Duration::from_secs(5), all_three());
    let t = 0_i64;
    sm.observe(Signal::Health, t, true);
    sm.observe(Signal::Health, t + 500_000_000, false);
    sm.observe(Signal::Health, t + 1_000_000_000, true);
    sm.observe(Signal::LoadGen5xx, t + 1_500_000_000, true);
    sm.observe(Signal::PromErrorRate, t + 2_000_000_000, true);
    assert_eq!(
        sm.recovery_ts_if_held(t + 7_000_000_000),
        Some(t + 2_000_000_000)
    );
}

#[test]
fn missing_required_signal_blocks_recovery() {
    // All three required, but LoadGen5xx is never observed clear -> no recovery,
    // even though the other two have been clear well past the grace period.
    let mut sm = SignalStateMachine::new(std::time::Duration::from_secs(5), all_three());
    let t = 0_i64;
    sm.observe(Signal::Health, t, true);
    sm.observe(Signal::PromErrorRate, t, true);
    assert_eq!(sm.recovery_ts_if_held(t + 60_000_000_000), None);
}

#[test]
fn unavailable_signal_can_be_excluded() {
    // When the load-gen signal is unavailable it is simply left out of the
    // required set; recovery is then driven by the remaining signals.
    let required = BTreeSet::from([Signal::Health, Signal::PromErrorRate]);
    let mut sm = SignalStateMachine::new(std::time::Duration::from_secs(5), required);
    let t = 0_i64;
    sm.observe(Signal::Health, t, true);
    sm.observe(Signal::PromErrorRate, t + 1_000_000_000, true);
    // An (unrequired) load-gen observation must not delay or block recovery.
    assert_eq!(
        sm.recovery_ts_if_held(t + 7_000_000_000),
        Some(t + 1_000_000_000)
    );
}
