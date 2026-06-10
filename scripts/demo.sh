#!/usr/bin/env bash
# Live demo of the OTel Multi-Source Correlation Engine.
#
# Narrated arc: healthy system -> inject a real 800ms latency fault on the
# notifications dependency -> watch the telemetry react -> hand a slow trace to
# the engine, which auto-pinpoints the culprit -> reproducible eval coda.
# The injected fault is always removed on exit (even on Ctrl-C).
#
#   ./scripts/demo.sh                 # interactive: pauses between acts
#   DEMO_NOPAUSE=1 ./scripts/demo.sh  # run straight through (e.g. to rehearse)
#
# Prereq: the research stack is up:
#   docker compose -f compose/docker-compose.yaml --profile research up -d
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TX=localhost:8003; NOTIF=localhost:8004; ACC=localhost:8002; TOXI=localhost:8474
TEMPO=localhost:3200; ENGINE=localhost:8500; PROM=localhost:9090
GRAFANA=localhost:3001; ZIPKIN=localhost:9411

bold(){ printf '\033[1m%s\033[0m\n' "$*"; }
dim(){  printf '\033[2m%s\033[0m\n' "$*"; }
act(){  printf '\n\033[1;36m%s\033[0m\n' "$*"; }
pause(){ [ -n "${DEMO_NOPAUSE:-}" ] && return 0; printf '\033[33m▶ press enter…\033[0m '; read -r _ || true; }
# print a URL and (unless DEMO_NOOPEN) open it in the default browser
open_url(){
  printf '   \033[34m↗ open %s\033[0m\n' "$1"
  [ -n "${DEMO_NOOPEN:-}" ] && return 0
  if command -v open >/dev/null 2>&1; then open "$1" >/dev/null 2>&1 || true
  elif command -v xdg-open >/dev/null 2>&1; then xdg-open "$1" >/dev/null 2>&1 || true; fi
}

LOAD_PID=""
cleanup(){
  [ -n "$LOAD_PID" ] && kill "$LOAD_PID" 2>/dev/null
  curl -s -X DELETE "http://$TOXI/proxies/smtp-fake/toxics/smtp-latency" >/dev/null 2>&1
  printf '\n'; dim "cleaned up — fault removed, load stopped"
}
trap cleanup EXIT INT TERM

# p99 latency (ms) for a service, from the collector's spanmetrics
p99(){
  curl -s "http://$PROM/api/v1/query" --data-urlencode \
    "query=histogram_quantile(0.99, sum by (le) (rate(duration_milliseconds_bucket{service_name=\"$1\"}[1m])))" -G 2>/dev/null \
    | python3 -c 'import sys,json;r=json.load(sys.stdin)["data"]["result"];print(("%.0f" % float(r[0]["value"][1])) if r else "n/a")' 2>/dev/null || echo "n/a"
}

# ---------------------------------------------------------------- preflight
bold "OTel Multi-Source Correlation Engine — live demo"
dim  "Grafana: http://$GRAFANA   Zipkin: http://$ZIPKIN   Engine API: http://$ENGINE"
for hp in "$TX/health" "$NOTIF/health" "$ENGINE/healthz"; do
  if ! curl -sf -o /dev/null --max-time 3 "http://$hp"; then
    echo "!! http://$hp unreachable — bring the stack up first:"
    echo "   docker compose -f compose/docker-compose.yaml --profile research up -d"
    exit 1
  fi
done
curl -s -o /dev/null -X POST "http://$ACC/accounts" -H 'content-type: application/json' -d '{"id":"a1","owner":"alice"}'
curl -s -o /dev/null -X POST "http://$ACC/accounts" -H 'content-type: application/json' -d '{"id":"a2","owner":"bob"}'

# steady background load so the dashboards and traces are live
( while true; do
    curl -s -o /dev/null --max-time 5 -X POST "http://$TX/transactions" \
      -H 'content-type: application/json' -d '{"from":"a1","to":"a2","amount":100}'
  done ) &
LOAD_PID=$!
dim "load generator running (pid $LOAD_PID)"
sleep 8

# ---------------------------------------------------------------- 1: healthy
act "1/4  Healthy system"
echo "   notifications p99 = $(p99 notifications) ms    transactions p99 = $(p99 transactions) ms"
echo "   The service dependency graph (transactions → accounts, notifications):"
open_url "http://$ZIPKIN/zipkin/dependency"
pause

# ---------------------------------------------------------------- 2: fault
act "2/4  Inject an 800ms SMTP-latency fault on the notifications dependency"
curl -s -X POST "http://$TOXI/proxies/smtp-fake/toxics" \
  -d '{"name":"smtp-latency","type":"latency","stream":"downstream","toxicity":1.0,"attributes":{"latency":800,"jitter":150}}' >/dev/null
echo "   Grafana 'Services' dashboard — watch notifications p99 spike (/notify):"
open_url "http://$GRAFANA/d/services?from=now-15m&to=now&refresh=5s"
printf '   notifications p99 climbing: '
for _ in $(seq 1 8); do sleep 4; printf '%sms ' "$(p99 notifications)"; done
printf '\n'
pause

# ---------------------------------------------------------------- 3: trace
act "3/4  Find a slow trace caused by the fault"
TID=""
for _ in $(seq 1 15); do
  TID=$(curl -s "http://$TEMPO/api/search" \
    --data-urlencode 'q={ resource.service.name="notifications" && duration>500ms }' \
    --data-urlencode "start=$(($(date +%s)-300))" --data-urlencode "end=$(($(date +%s)+10))" \
    --data-urlencode "limit=1" -G 2>/dev/null \
    | python3 -c 'import sys,json;t=json.load(sys.stdin).get("traces",[]);print(t[0]["traceID"] if t else "")' 2>/dev/null)
  [ -n "$TID" ] && break
  sleep 3
done
if [ -z "$TID" ]; then
  echo "   (no slow trace yet — let load run a few more seconds)"
else
  echo "   trace: $TID  — the long span is notifications:/notify; transactions:create"
  echo "   is long only because it is *waiting* on it:"
  open_url "http://$ZIPKIN/zipkin/traces/$TID"
fi
pause

# ---------------------------------------------------------------- 4: engine
act "4/4  Hand the trace to the correlation engine — no human looks at it"
if [ -n "$TID" ]; then
  curl -s -X POST "http://$ENGINE/correlate/trace" -H 'content-type: application/json' \
    -d "{\"trace_id\":\"$TID\"}" \
    | python3 -c '
import sys, json
d = json.load(sys.stdin)
print("   services in incident:", ", ".join(s["name"] for s in d["services"]))
print("   ranked suspects:")
for s in d["suspects"]:
    eb = s["evidence_breakdown"]
    print("     #%d  %-14s score=%.2f  (latency=%.2f  error=%.2f)" % (
        s["rank"], s["service"], s["score"],
        eb.get("direct_latency_weight", 0.0), eb.get("direct_error_weight", 0.0)))
print("   elapsed:", d["elapsed_ms"], "ms")
' 2>/dev/null || echo "   (engine call failed)"
  echo "   → #1 is notifications, on self-time latency evidence: the slow worker,"
  echo "     not the caller blocked waiting on it."
fi
pause

# ---------------------------------------------------------------- coda
act "Reproducible evaluation — the research artifact"
( cd "$REPO" && docker compose -f compose/docker-compose.yaml exec -T eval-harness \
    eval --eval /data/eval_runs.db report --tag suite-baseline 2>/dev/null ) \
  || dim "(no eval yet — run: eval ... run --suite '/experiments/*.yaml' --tag suite-baseline)"
echo "   The eval scores in Grafana (composite by scenario × mode):"
open_url "http://$GRAFANA/d/eval"
[ -f "$REPO/results/scorecard.html" ] && open_url "file://$REPO/results/scorecard.html"

act "Done — removing the fault."
# cleanup() runs via the EXIT trap
