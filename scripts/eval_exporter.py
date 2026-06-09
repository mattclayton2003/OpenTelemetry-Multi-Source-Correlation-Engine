#!/usr/bin/env python3
"""Prometheus exporter for eval results.

Serves the contents of eval_runs.db as Prometheus gauges on :9112/metrics so the
engine's scores (composite, recall@k, precision@3) live in Grafana alongside the
telemetry. Stdlib only; the DB is re-read on every scrape, so a new eval run
shows up without restarting the exporter.

    eval_composite{tag,experiment,mode}
    eval_recall_at_1 / _3 / _5{tag,experiment,mode}
    eval_precision_at_3{tag,experiment,mode}
    eval_elapsed_ms{tag,experiment,mode}
    eval_run_started_seconds{tag}        # run start (epoch seconds)
    eval_run_info{tag,config_hash}       # always 1, carries metadata labels
"""
import os
import sqlite3
from http.server import BaseHTTPRequestHandler, HTTPServer

DB = os.environ.get("EVAL_DB", "/data/eval_runs.db")
PORT = int(os.environ.get("PORT", "9112"))


def esc(v: str) -> str:
    return str(v).replace("\\", "\\\\").replace('"', '\\"').replace("\n", " ")


def render() -> str:
    if not os.path.exists(DB):
        return "# eval_runs.db not found yet\neval_exporter_up 1\n"
    # Normal open (we only ever SELECT): a read-only/immutable open can't read a
    # WAL database whose -shm/-wal sidecars it isn't allowed to touch. The data
    # volume is mounted writable so SQLite can manage the shared-memory index.
    db = sqlite3.connect(DB, timeout=2)
    db.row_factory = sqlite3.Row
    out = ["eval_exporter_up 1"]
    runs = {
        r["eval_run_id"]: r
        for r in db.execute(
            "SELECT eval_run_id, tag, started_at, config_hash FROM eval_runs"
        ).fetchall()
    }
    # run-level metadata
    out.append("# TYPE eval_run_started_seconds gauge")
    out.append("# TYPE eval_run_info gauge")
    for r in runs.values():
        tag = esc(r["tag"])
        out.append(f'eval_run_started_seconds{{tag="{tag}"}} {(r["started_at"] or 0)/1e9:.0f}')
        out.append(f'eval_run_info{{tag="{tag}",config_hash="{esc(r["config_hash"])}"}} 1')

    metrics = {
        "eval_composite": "composite",
        "eval_recall_at_1": "recall_at_1",
        "eval_recall_at_3": "recall_at_3",
        "eval_recall_at_5": "recall_at_5",
        "eval_precision_at_3": "precision_at_3",
        "eval_elapsed_ms": "elapsed_ms",
    }
    for name in metrics:
        out.append(f"# TYPE {name} gauge")
    rows = db.execute(
        "SELECT eval_run_id, experiment_id, invocation_mode, "
        "recall_at_1, recall_at_3, recall_at_5, precision_at_3, composite, elapsed_ms "
        "FROM eval_results"
    ).fetchall()
    for row in rows:
        run = runs.get(row["eval_run_id"])
        if run is None:
            continue
        labels = (
            f'tag="{esc(run["tag"])}",'
            f'experiment="{esc(row["experiment_id"])}",'
            f'mode="{esc(row["invocation_mode"])}"'
        )
        for name, col in metrics.items():
            out.append(f"{name}{{{labels}}} {row[col]}")
    db.close()
    return "\n".join(out) + "\n"


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path.rstrip("/") not in ("/metrics", ""):
            self.send_response(404)
            self.end_headers()
            return
        try:
            body = render().encode()
        except Exception as e:  # never let a bad DB take the exporter down
            body = f"eval_exporter_up 0\n# error: {e}\n".encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/plain; version=0.0.4")
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *_):  # quiet
        pass


if __name__ == "__main__":
    print(f"eval-exporter serving {DB} on :{PORT}/metrics", flush=True)
    HTTPServer(("0.0.0.0", PORT), Handler).serve_forever()
