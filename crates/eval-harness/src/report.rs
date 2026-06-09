use sqlx::SqlitePool;
use std::fmt::Write;

pub async fn print_for_tag(eval: &SqlitePool, tag: &str) -> anyhow::Result<()> {
    let run: Option<(String, i64, i64, String, String)> = sqlx::query_as(
        "SELECT eval_run_id, started_at, ended_at, config_hash, scoring_toml_hash FROM eval_runs WHERE tag = ? ORDER BY started_at DESC LIMIT 1"
    ).bind(tag).fetch_optional(eval).await?;
    let Some((eval_run_id, _started, _ended, cfg, scoring)) = run else {
        println!("no eval run with tag {tag}");
        return Ok(());
    };

    let mut s = String::new();
    writeln!(s, "# Eval Report — {tag}")?;
    writeln!(s, "engine cfg `{cfg}` · scoring `{scoring}`\n")?;

    let head: (f64, f64, f64, f64, f64, f64) = sqlx::query_as(
        "SELECT AVG(recall_at_1), AVG(recall_at_3), AVG(recall_at_5),
                AVG(precision_at_3), AVG(composite), AVG(elapsed_ms)
         FROM eval_results WHERE eval_run_id = ?",
    )
    .bind(&eval_run_id)
    .fetch_one(eval)
    .await?;
    writeln!(s, "## Headline")?;
    writeln!(
        s,
        "recall@1 {:.2}  recall@3 {:.2}  recall@5 {:.2}",
        head.0, head.1, head.2
    )?;
    writeln!(
        s,
        "precision@3 {:.2}  composite {:.2}  elapsed_avg {:.0}ms\n",
        head.3, head.4, head.5
    )?;

    let modes: Vec<(String, i64, f64, f64)> = sqlx::query_as(
        "SELECT invocation_mode, COUNT(*), AVG(recall_at_3), AVG(composite)
         FROM eval_results WHERE eval_run_id = ? GROUP BY invocation_mode",
    )
    .bind(&eval_run_id)
    .fetch_all(eval)
    .await?;
    writeln!(s, "## By invocation mode")?;
    writeln!(s, "| mode | n | recall@3 | composite |")?;
    writeln!(s, "|---|---|---|---|")?;
    for (m, n, r, c) in modes {
        writeln!(s, "| {m} | {n} | {r:.2} | {c:.2} |")?;
    }
    writeln!(s)?;

    let misses: Vec<(String, String)> = sqlx::query_as(
        "SELECT experiment_id, invocation_mode FROM eval_results WHERE eval_run_id = ? AND recall_at_3 = 0.0"
    ).bind(&eval_run_id).fetch_all(eval).await?;
    writeln!(s, "## Misses (recall@3 = 0)")?;
    if misses.is_empty() {
        writeln!(s, "(none)")?;
    }
    for (e, m) in misses {
        writeln!(s, "- {e} ({m})")?;
    }
    writeln!(s)?;

    std::fs::create_dir_all(format!("results/{tag}"))?;
    std::fs::write(format!("results/{tag}/report.md"), &s)?;
    println!("{s}");
    Ok(())
}

pub async fn reproduce(eval: &SqlitePool, id: &str) -> anyhow::Result<()> {
    // Read stored config_json + scoring_hash + composite per row; recompute and diff.
    // For v1 stub: report what would be reproduced. Full re-invocation of engine
    // requires running stack (Docker). Same pattern as other Docker-deferred tasks.
    let originals: Vec<(String, String, f64)> = sqlx::query_as(
        "SELECT experiment_id, invocation_mode, composite FROM eval_results WHERE eval_run_id = ?",
    )
    .bind(id)
    .fetch_all(eval)
    .await?;
    if originals.is_empty() {
        println!("no eval_run {id} found");
        return Ok(());
    }
    println!("eval_run_id: {id}");
    println!("{} rows would be re-scored:", originals.len());
    for (exp_id, mode, comp) in originals {
        println!("  {exp_id} ({mode}) original composite={comp:.4}");
    }
    println!("\n(Full re-invocation requires Docker stack — run with `cargo run -p eval-harness -- reproduce` against a live env.)");
    Ok(())
}
