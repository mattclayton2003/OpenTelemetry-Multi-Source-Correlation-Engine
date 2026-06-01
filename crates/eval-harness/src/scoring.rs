use serde::{Deserialize, Serialize};

pub fn recall_at_k(suspects: &[String], primary: &str, k: usize) -> f64 {
    if suspects.iter().take(k).any(|s| s == primary) { 1.0 } else { 0.0 }
}

pub fn precision_at_k(suspects: &[String], primary: &[&str], blast: &[&str], k: usize) -> f64 {
    let denom = k.max(1) as f64;
    let hits = suspects.iter().take(k).filter(|s|
        primary.contains(&s.as_str()) || blast.contains(&s.as_str())
    ).count() as f64;
    hits / denom
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Weights {
    pub recall: f64, pub precision: f64, pub completeness: f64,
    pub time: f64, pub fp_penalty: f64,
}
impl Default for Weights {
    fn default() -> Self { Self { recall: 0.50, precision: 0.10, completeness: 0.25, time: 0.10, fp_penalty: 0.05 } }
}

pub struct ScoreInputs {
    pub recall_at_3: f64, pub precision_at_3: f64,
    pub completeness_mean: f64, pub elapsed_ms: i64,
    pub normalized_clean_fps: f64,
}

pub fn composite(s: &ScoreInputs, w: &Weights) -> f64 {
    let time_term = (1.0 - (s.elapsed_ms as f64) / 10_000.0).max(0.0);
    w.recall * s.recall_at_3
      + w.precision * s.precision_at_3
      + w.completeness * s.completeness_mean
      + w.time * time_term
      - w.fp_penalty * s.normalized_clean_fps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recall_at_k_is_1_when_primary_in_top_k() {
        let suspects = vec!["a","b","c","d"].into_iter().map(String::from).collect::<Vec<_>>();
        assert_eq!(recall_at_k(&suspects, "c", 3), 1.0);
        assert_eq!(recall_at_k(&suspects, "d", 3), 0.0);
    }

    #[test]
    fn precision_at_k_counts_blast_radius() {
        let suspects = vec!["a","b","c"].into_iter().map(String::from).collect::<Vec<_>>();
        let truth = ["a"]; let blast = ["b"];
        assert_eq!(precision_at_k(&suspects, &truth, &blast, 3), 2.0/3.0);
    }

    #[test]
    fn composite_combines_components_per_spec() {
        let s = ScoreInputs {
            recall_at_3: 1.0, precision_at_3: 1.0,
            completeness_mean: 0.5, elapsed_ms: 1000,
            normalized_clean_fps: 0.0,
        };
        let c = composite(&s, &Weights::default());
        // 0.50*1 + 0.10*1 + 0.25*0.5 + 0.10*0.9 + 0 = 0.815
        assert!((c - 0.815).abs() < 1e-6);
    }
}
