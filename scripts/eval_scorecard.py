#!/usr/bin/env python3
"""Render eval results (eval_runs.db) as a self-contained HTML scorecard.

Reads the eval_runs / eval_results tables and emits a single static HTML file:
one section per eval run (tag + timestamp), with a per-experiment x mode table
whose cells are colour-coded by composite score (red -> green). No server,
plugin, or rebuild required:

    python3 scripts/eval_scorecard.py [--db data/eval_runs.db] [--out results/scorecard.html]
"""
import argparse
import datetime as dt
import html
import sqlite3


def color(score: float) -> str:
    # 0.0 -> red, 0.5 -> amber, 1.0 -> green
    s = max(0.0, min(1.0, score))
    hue = 120 * s
    return f"hsl({hue:.0f}, 70%, 45%)"


def fmt_ts(ns: int) -> str:
    if not ns:
        return "?"
    return dt.datetime.fromtimestamp(ns / 1e9).strftime("%Y-%m-%d %H:%M:%S")


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--db", default="data/eval_runs.db")
    ap.add_argument("--out", default="results/scorecard.html")
    args = ap.parse_args()

    db = sqlite3.connect(args.db)
    db.row_factory = sqlite3.Row
    runs = db.execute(
        "SELECT eval_run_id, tag, started_at, config_hash FROM eval_runs ORDER BY started_at DESC"
    ).fetchall()

    sections = []
    for run in runs:
        rows = db.execute(
            """SELECT experiment_id, invocation_mode, recall_at_1, recall_at_3,
                      precision_at_3, composite
               FROM eval_results WHERE eval_run_id = ?
               ORDER BY experiment_id, invocation_mode""",
            (run["eval_run_id"],),
        ).fetchall()
        if not rows:
            continue
        # pivot: experiment -> {mode: row}
        by_exp: dict[str, dict[str, sqlite3.Row]] = {}
        for r in rows:
            by_exp.setdefault(r["experiment_id"], {})[r["invocation_mode"]] = r
        modes = sorted({r["invocation_mode"] for r in rows})

        avg_comp = sum(r["composite"] for r in rows) / len(rows)
        avg_r3 = sum(r["recall_at_3"] for r in rows) / len(rows)

        head = (
            f"<div class=head><span class=tag>{html.escape(run['tag'])}</span>"
            f"<span class=meta>{fmt_ts(run['started_at'])} &middot; "
            f"composite {avg_comp:.2f} &middot; recall@3 {avg_r3:.2f} &middot; "
            f"{html.escape(run['config_hash'])}</span></div>"
        )

        header_cells = "".join(
            f"<th colspan=2>{html.escape(m)}</th>" for m in modes
        )
        sub = "".join("<th>composite</th><th>r@1 / r@3 / p@3</th>" for _ in modes)
        body = ""
        for exp in sorted(by_exp):
            cells = ""
            for m in modes:
                r = by_exp[exp].get(m)
                if r is None:
                    cells += "<td>-</td><td>-</td>"
                    continue
                c = r["composite"]
                cells += (
                    f"<td style='background:{color(c)}'>{c:.2f}</td>"
                    f"<td class=metrics>{r['recall_at_1']:.0f} / "
                    f"{r['recall_at_3']:.0f} / {r['precision_at_3']:.2f}</td>"
                )
            body += f"<tr><th class=exp>{html.escape(exp)}</th>{cells}</tr>"

        sections.append(
            f"<section>{head}<table>"
            f"<tr><th rowspan=2 class=exp>experiment</th>{header_cells}</tr>"
            f"<tr>{sub}</tr>{body}</table></section>"
        )

    doc = f"""<!doctype html><meta charset=utf-8>
<title>OTel Correlation Engine — Eval Scorecard</title>
<style>
 body{{font:14px/1.4 -apple-system,Segoe UI,Roboto,sans-serif;margin:2rem;color:#1a1a1a;background:#fafafa}}
 h1{{font-size:1.3rem}}
 section{{background:#fff;border:1px solid #e2e2e2;border-radius:8px;margin:1rem 0;padding:1rem;box-shadow:0 1px 3px rgba(0,0,0,.05)}}
 .head{{display:flex;justify-content:space-between;align-items:baseline;margin-bottom:.5rem}}
 .tag{{font-weight:700;font-size:1.05rem}}
 .meta{{color:#666;font-size:.85rem;font-family:ui-monospace,monospace}}
 table{{border-collapse:collapse;width:100%}}
 th,td{{border:1px solid #e6e6e6;padding:.35rem .6rem;text-align:center}}
 td{{color:#fff;font-weight:600}}
 .exp{{text-align:left;background:#f4f4f4;color:#222;font-weight:600}}
 .metrics{{background:#fff;color:#555;font-weight:400;font-family:ui-monospace,monospace;font-size:.82rem}}
 .legend{{font-size:.8rem;color:#666;margin:.5rem 0}}
</style>
<h1>OTel Correlation Engine — Eval Scorecard</h1>
<p class=legend>Cells coloured by <b>composite</b> score (red 0 &rarr; green 1).
Sub-cell shows <code>recall@1 / recall@3 / precision@3</code>. Newest run first.</p>
{''.join(sections) if sections else '<p>No eval runs found.</p>'}
"""
    import os
    os.makedirs(os.path.dirname(args.out), exist_ok=True)
    with open(args.out, "w") as f:
        f.write(doc)
    print(f"wrote {args.out} ({len(sections)} run(s))")


if __name__ == "__main__":
    main()
