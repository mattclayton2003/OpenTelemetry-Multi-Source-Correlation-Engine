//! Determinism canary — re-runs `eval reproduce <id>` and verifies
//! the recomputed composite scores match the stored values within
//! ε = 0.001.
//!
//! Implementation requires a live Docker stack to re-invoke the
//! engine. For v1, this module declares the public API contract;
//! the actual canary runner lives in CI (Phase 8 reproduce.yml).

pub const EPSILON: f64 = 0.001;

pub fn within_epsilon(stored: f64, recomputed: f64) -> bool {
    (stored - recomputed).abs() < EPSILON
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn within_epsilon_works() {
        assert!(within_epsilon(0.5, 0.5005));
        assert!(within_epsilon(0.5, 0.4995));
        assert!(!within_epsilon(0.5, 0.6));
    }
}
