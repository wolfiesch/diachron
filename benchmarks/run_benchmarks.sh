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
THRESHOLD_CLI_COLD_START_MS=50
THRESHOLD_SEARCH_MS=100
THRESHOLD_HOOK_MS=20
THRESHOLD_MEMORY_MB=200

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

print_result "CLI cold start" "$CLI_MS" "ms" "$THRESHOLD_CLI_COLD_START_MS"
JSON_RESULTS+=("\"cli_cold_start_ms\": $CLI_MS")

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

print_result "Daemon IPC" "$IPC_MS" "ms"
JSON_RESULTS+=("\"daemon_ipc_ms\": $IPC_MS")

# ============================================================================
# Benchmark 3: Search Latency
# ============================================================================
print_header "Search Latency (semantic + FTS)"

SEARCH_RESULT=$(hyperfine --warmup 3 --runs 10 --export-json /tmp/bench_search.json \
    "$DIACHRON_CLI search 'authentication' --limit 10" 2>&1 | grep -E "Time \(mean" | head -1)

SEARCH_MS=$(jq '.results[0].mean * 1000' /tmp/bench_search.json 2>/dev/null || echo "N/A")
SEARCH_MS=$(printf "%.1f" "$SEARCH_MS")

print_result "Search latency" "$SEARCH_MS" "ms" "$THRESHOLD_SEARCH_MS"
JSON_RESULTS+=("\"search_latency_ms\": $SEARCH_MS")

# ============================================================================
# Benchmark 4: Hook Capture Latency
# ============================================================================
print_header "Hook Capture Latency"

if [ -f "$DIACHRON_HOOK" ]; then
    HOOK_RESULT=$(hyperfine --warmup 3 --runs 10 --export-json /tmp/bench_hook.json \
        "echo '{\"tool_name\":\"Write\",\"file_path\":\"/tmp/test.txt\",\"diff_summary\":\"+1 line\"}' | $DIACHRON_HOOK" 2>&1)

    HOOK_MS=$(jq '.results[0].mean * 1000' /tmp/bench_hook.json 2>/dev/null || echo "N/A")
    HOOK_MS=$(printf "%.1f" "$HOOK_MS")

    print_result "Hook capture" "$HOOK_MS" "ms" "$THRESHOLD_HOOK_MS"
    JSON_RESULTS+=("\"hook_capture_ms\": $HOOK_MS")
else
    echo "  Warning: Hook binary not found, skipping"
    JSON_RESULTS+=("\"hook_capture_ms\": null")
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
