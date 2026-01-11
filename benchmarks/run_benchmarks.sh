#!/bin/bash
# Diachron v0.2.0 Benchmark Suite
# Measures latency, memory, and storage metrics
#
# Usage: ./run_benchmarks.sh [--ci] [--json]
#   --ci    Exit with non-zero if benchmarks exceed thresholds
#   --json  Output results as JSON

set -e

# Configuration
SKILL_DIR="$HOME/.claude/skills/diachron"
DIACHRON_CLI="$SKILL_DIR/rust/target/release/diachron"
DIACHRON_HOOK="$SKILL_DIR/rust/target/release/diachron-hook"
DIACHROND="$SKILL_DIR/rust/target/release/diachrond"

# Thresholds for CI (fail if exceeded)
# P95/P99 thresholds are baseline values with headroom to reduce noise.
THRESHOLD_CLI_COLD_START_MS=50
THRESHOLD_CLI_COLD_START_P95_MS=325
THRESHOLD_CLI_COLD_START_P99_MS=330
THRESHOLD_SEARCH_MS=100
THRESHOLD_SEARCH_P95_MS=640
THRESHOLD_SEARCH_P99_MS=805
THRESHOLD_HOOK_MS=20
THRESHOLD_HOOK_P95_MS=20
THRESHOLD_HOOK_P99_MS=34
THRESHOLD_MEMORY_MB=200
THRESHOLD_IPC_P95_MS=145
THRESHOLD_IPC_P99_MS=210

# Parse arguments
CI_MODE=false
JSON_OUTPUT=false
for arg in "$@"; do
    case $arg in
        --ci) CI_MODE=true ;;
        --json) JSON_OUTPUT=true ;;
    esac
done

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

print_header() {
    if [ "$JSON_OUTPUT" != "true" ]; then
        echo -e "\n${BLUE}═══════════════════════════════════════════════════════════${NC}"
        echo -e "${BLUE}  $1${NC}"
        echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}\n"
    fi
}

print_result() {
    local name="$1"
    local value="$2"
    local unit="$3"
    local threshold="$4"
    local status="PASS"

    if [ -n "$threshold" ] && [ "$(echo "$value > $threshold" | bc -l)" -eq 1 ]; then
        status="FAIL"
    fi

    if [ "$JSON_OUTPUT" != "true" ]; then
        if [ "$status" = "PASS" ]; then
            echo -e "  ${GREEN}✓${NC} $name: ${value}${unit}"
        else
            echo -e "  ${RED}✗${NC} $name: ${value}${unit} (threshold: ${threshold}${unit})"
        fi
    fi

    # Track failures
    if [ "$status" = "FAIL" ]; then
        FAILURES=$((FAILURES + 1))
    fi
}

get_percentiles() {
    local path="$1"
    python3 - "$path" <<'PY'
import json
import math
import sys
from pathlib import Path

path = Path(sys.argv[1])
if not path.exists() or path.stat().st_size == 0:
    print("N/A N/A N/A")
    raise SystemExit(0)

data = json.loads(path.read_text())
times = data.get("results", [{}])[0].get("times", [])
if not times:
    print("N/A N/A N/A")
    raise SystemExit(0)

def pct(vals, p):
    vals = sorted(vals)
    k = (len(vals) - 1) * (p / 100)
    f = math.floor(k)
    c = math.ceil(k)
    if f == c:
        return vals[int(k)]
    return vals[f] + (vals[c] - vals[f]) * (k - f)

p50 = pct(times, 50) * 1000
p95 = pct(times, 95) * 1000
p99 = pct(times, 99) * 1000
print(f"{p50:.1f} {p95:.1f} {p99:.1f}")
PY
}

normalize_json_number() {
    local value="$1"
    if [ "$value" = "N/A" ]; then
        echo "null"
    else
        echo "$value"
    fi
}

# Initialize
FAILURES=0

# Check prerequisites
if ! command -v hyperfine &> /dev/null; then
    echo "Error: hyperfine is required. Install with: brew install hyperfine"
    exit 1
fi

if [ ! -f "$DIACHRON_CLI" ]; then
    echo "Error: diachron CLI not found at $DIACHRON_CLI"
    echo "Build with: cd $SKILL_DIR/rust && cargo build --release"
    exit 1
fi

# JSON output array
JSON_RESULTS=()

# ============================================================================
# Benchmark 1: CLI Cold Start
# ============================================================================
print_header "CLI Cold Start (no daemon communication)"

CLI_RESULT=$(hyperfine --warmup 2 --runs 10 --export-json /tmp/bench_cli.json \
    "$DIACHRON_CLI --help" 2>&1 | grep -E "Time \(mean" | head -1)

CLI_MS=$(jq '.results[0].mean * 1000' /tmp/bench_cli.json 2>/dev/null || echo "N/A")
CLI_MS=$(printf "%.1f" "$CLI_MS")
read -r CLI_P50 CLI_P95 CLI_P99 < <(get_percentiles /tmp/bench_cli.json)
CLI_P95_JSON=$(normalize_json_number "$CLI_P95")
CLI_P99_JSON=$(normalize_json_number "$CLI_P99")

print_result "CLI cold start" "$CLI_MS" "ms" "$THRESHOLD_CLI_COLD_START_MS"
if [ "$CLI_P95" != "N/A" ]; then
    print_result "CLI cold start p95" "$CLI_P95" "ms" "$THRESHOLD_CLI_COLD_START_P95_MS"
    print_result "CLI cold start p99" "$CLI_P99" "ms" "$THRESHOLD_CLI_COLD_START_P99_MS"
fi
JSON_RESULTS+=("\"cli_cold_start_ms\": $CLI_MS")
JSON_RESULTS+=("\"cli_cold_start_p95_ms\": $CLI_P95_JSON")
JSON_RESULTS+=("\"cli_cold_start_p99_ms\": $CLI_P99_JSON")

# ============================================================================
# Benchmark 2: Daemon IPC Round-trip
# ============================================================================
print_header "Daemon IPC (ping/status)"

# Check if daemon is running
if ! "$DIACHRON_CLI" daemon status &>/dev/null; then
    echo "  Warning: Daemon not running, starting it..."
    "$DIACHRON_CLI" daemon start
    sleep 2
fi

IPC_RESULT=$(hyperfine --warmup 3 --runs 10 --export-json /tmp/bench_ipc.json \
    "$DIACHRON_CLI daemon status" 2>&1 | grep -E "Time \(mean" | head -1)

IPC_MS=$(jq '.results[0].mean * 1000' /tmp/bench_ipc.json 2>/dev/null || echo "N/A")
IPC_MS=$(printf "%.1f" "$IPC_MS")
read -r IPC_P50 IPC_P95 IPC_P99 < <(get_percentiles /tmp/bench_ipc.json)
IPC_P95_JSON=$(normalize_json_number "$IPC_P95")
IPC_P99_JSON=$(normalize_json_number "$IPC_P99")

print_result "Daemon IPC" "$IPC_MS" "ms"
if [ "$IPC_P95" != "N/A" ]; then
    print_result "Daemon IPC p95" "$IPC_P95" "ms" "$THRESHOLD_IPC_P95_MS"
    print_result "Daemon IPC p99" "$IPC_P99" "ms" "$THRESHOLD_IPC_P99_MS"
fi
JSON_RESULTS+=("\"daemon_ipc_ms\": $IPC_MS")
JSON_RESULTS+=("\"daemon_ipc_p95_ms\": $IPC_P95_JSON")
JSON_RESULTS+=("\"daemon_ipc_p99_ms\": $IPC_P99_JSON")

# ============================================================================
# Benchmark 3: Search Latency
# ============================================================================
print_header "Search Latency (semantic + FTS)"

SEARCH_RESULT=$(hyperfine --warmup 3 --runs 10 --export-json /tmp/bench_search.json \
    "$DIACHRON_CLI search 'authentication' --limit 10" 2>&1 | grep -E "Time \(mean" | head -1)

SEARCH_MS=$(jq '.results[0].mean * 1000' /tmp/bench_search.json 2>/dev/null || echo "N/A")
SEARCH_MS=$(printf "%.1f" "$SEARCH_MS")
read -r SEARCH_P50 SEARCH_P95 SEARCH_P99 < <(get_percentiles /tmp/bench_search.json)
SEARCH_P95_JSON=$(normalize_json_number "$SEARCH_P95")
SEARCH_P99_JSON=$(normalize_json_number "$SEARCH_P99")

print_result "Search latency" "$SEARCH_MS" "ms" "$THRESHOLD_SEARCH_MS"
if [ "$SEARCH_P95" != "N/A" ]; then
    print_result "Search latency p95" "$SEARCH_P95" "ms" "$THRESHOLD_SEARCH_P95_MS"
    print_result "Search latency p99" "$SEARCH_P99" "ms" "$THRESHOLD_SEARCH_P99_MS"
fi
JSON_RESULTS+=("\"search_latency_ms\": $SEARCH_MS")
JSON_RESULTS+=("\"search_latency_p95_ms\": $SEARCH_P95_JSON")
JSON_RESULTS+=("\"search_latency_p99_ms\": $SEARCH_P99_JSON")

# ============================================================================
# Benchmark 4: Hook Capture Latency
# ============================================================================
print_header "Hook Capture Latency"

if [ -f "$DIACHRON_HOOK" ]; then
    HOOK_RESULT=$(hyperfine --warmup 3 --runs 10 --export-json /tmp/bench_hook.json \
        "echo '{\"tool_name\":\"Write\",\"file_path\":\"/tmp/test.txt\",\"diff_summary\":\"+1 line\"}' | $DIACHRON_HOOK" 2>&1)

    HOOK_MS=$(jq '.results[0].mean * 1000' /tmp/bench_hook.json 2>/dev/null || echo "N/A")
    HOOK_MS=$(printf "%.1f" "$HOOK_MS")
    read -r HOOK_P50 HOOK_P95 HOOK_P99 < <(get_percentiles /tmp/bench_hook.json)
    HOOK_P95_JSON=$(normalize_json_number "$HOOK_P95")
    HOOK_P99_JSON=$(normalize_json_number "$HOOK_P99")

    print_result "Hook capture" "$HOOK_MS" "ms" "$THRESHOLD_HOOK_MS"
    if [ "$HOOK_P95" != "N/A" ]; then
        print_result "Hook capture p95" "$HOOK_P95" "ms" "$THRESHOLD_HOOK_P95_MS"
        print_result "Hook capture p99" "$HOOK_P99" "ms" "$THRESHOLD_HOOK_P99_MS"
    fi
    JSON_RESULTS+=("\"hook_capture_ms\": $HOOK_MS")
    JSON_RESULTS+=("\"hook_capture_p95_ms\": $HOOK_P95_JSON")
    JSON_RESULTS+=("\"hook_capture_p99_ms\": $HOOK_P99_JSON")
else
    echo "  Warning: Hook binary not found, skipping"
    JSON_RESULTS+=("\"hook_capture_ms\": null")
    JSON_RESULTS+=("\"hook_capture_p95_ms\": null")
    JSON_RESULTS+=("\"hook_capture_p99_ms\": null")
fi

# ============================================================================
# Benchmark 5: Memory Usage
# ============================================================================
print_header "Memory Usage"

DAEMON_PID=$(pgrep -f diachrond 2>/dev/null || echo "")

if [ -n "$DAEMON_PID" ]; then
    MEMORY_KB=$(ps -o rss= -p "$DAEMON_PID" 2>/dev/null || echo "0")
    MEMORY_MB=$(echo "scale=1; $MEMORY_KB / 1024" | bc)

    print_result "Daemon RSS" "$MEMORY_MB" "MB" "$THRESHOLD_MEMORY_MB"
    JSON_RESULTS+=("\"daemon_memory_mb\": $MEMORY_MB")
else
    echo "  Warning: Daemon not running"
    JSON_RESULTS+=("\"daemon_memory_mb\": null")
fi

# ============================================================================
# Storage Statistics
# ============================================================================
print_header "Storage Statistics"

if [ -f "$HOME/.diachron/diachron.db" ]; then
    DB_SIZE=$(ls -lh "$HOME/.diachron/diachron.db" 2>/dev/null | awk '{print $5}')
    EVENTS=$(sqlite3 "$HOME/.diachron/diachron.db" "SELECT COUNT(*) FROM events" 2>/dev/null || echo "N/A")
    EXCHANGES=$(sqlite3 "$HOME/.diachron/diachron.db" "SELECT COUNT(*) FROM exchanges" 2>/dev/null || echo "N/A")

    if [ "$JSON_OUTPUT" != "true" ]; then
        echo "  Database size: $DB_SIZE"
        echo "  Events: $EVENTS"
        echo "  Exchanges: $EXCHANGES"
    fi

    JSON_RESULTS+=("\"database_size\": \"$DB_SIZE\"")
    JSON_RESULTS+=("\"events_count\": $EVENTS")
    JSON_RESULTS+=("\"exchanges_count\": $EXCHANGES")
fi

if [ -f "$HOME/.diachron/indexes/exchanges.usearch" ]; then
    INDEX_SIZE=$(ls -lh "$HOME/.diachron/indexes/exchanges.usearch" 2>/dev/null | awk '{print $5}')

    if [ "$JSON_OUTPUT" != "true" ]; then
        echo "  Vector index: $INDEX_SIZE"
    fi

    JSON_RESULTS+=("\"vector_index_size\": \"$INDEX_SIZE\"")
fi

# ============================================================================
# Summary
# ============================================================================
print_header "Summary"

if [ "$JSON_OUTPUT" = "true" ]; then
    echo "{"
    IFS=,
    echo "  ${JSON_RESULTS[*]}"
    echo "}"
else
    if [ "$FAILURES" -eq 0 ]; then
        echo -e "  ${GREEN}All benchmarks passed!${NC}"
    else
        echo -e "  ${RED}$FAILURES benchmark(s) exceeded thresholds${NC}"
    fi

    echo ""
    echo "  Results saved to /tmp/bench_*.json"
fi

# Exit with failure count for CI
if [ "$CI_MODE" = "true" ]; then
    exit $FAILURES
fi
