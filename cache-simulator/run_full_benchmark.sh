#!/bin/bash
#
# Full Cache Benchmark Script
# ===========================
# Reproduces the 33M+ request benchmark for cache algorithm comparison.
#
# Traffic profiles:
#   - video:  7.2M requests (500 RPS × 4h, 10K objects, large files)
#   - social: 14.4M requests (1000 RPS × 4h, 100K objects, extreme skew)
#   - web:    11.5M requests (800 RPS × 4h, 50K objects, moderate skew)
#   Total: ~33M requests
#
# Usage:
#   ./run_full_benchmark.sh                    # Run full benchmark (~15 min)
#   ./run_full_benchmark.sh --quick            # Quick validation (~1 min)
#   ./run_full_benchmark.sh --clean            # Clean traffic data first
#
# Output:
#   results/benchmark_YYYYMMDD_HHMMSS/         # Timestamped results
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BENCHMARK_DIR="$SCRIPT_DIR/results/benchmark_$TIMESTAMP"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

QUICK_MODE=false
CLEAN_MODE=false

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --quick) QUICK_MODE=true; shift ;;
        --clean) CLEAN_MODE=true; shift ;;
        -h|--help) head -20 "$0" | tail -15; exit 0 ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

echo ""
echo -e "${BOLD}${BLUE}════════════════════════════════════════════════════════════════${NC}"
echo -e "${BOLD}${BLUE} Cache Algorithm Benchmark${NC}"
echo -e "${BOLD}${BLUE}════════════════════════════════════════════════════════════════${NC}"
echo ""

# Create results directory
mkdir -p "$BENCHMARK_DIR"
echo -e "  ${CYAN}ℹ${NC} Results: $BENCHMARK_DIR"

# Record environment
GIT_COMMIT=$(cd "$REPO_ROOT" && git rev-parse --short HEAD 2>/dev/null || echo "unknown")
GIT_BRANCH=$(cd "$REPO_ROOT" && git branch --show-current 2>/dev/null || echo "unknown")

cat > "$BENCHMARK_DIR/environment.md" << EOF
# Benchmark Environment

- **Date:** $(date)
- **Git Branch:** $GIT_BRANCH
- **Git Commit:** $GIT_COMMIT
- **Rust:** $(rustc --version 2>/dev/null || echo "unknown")
- **CPU:** $(grep 'model name' /proc/cpuinfo 2>/dev/null | head -1 | cut -d: -f2 | xargs || echo "unknown")
- **Cores:** $(nproc 2>/dev/null || echo "unknown")
EOF

# Build
echo -e "  ${GREEN}▸${NC} Building release binary..."
(cd "$REPO_ROOT" && cargo build --release --features "std,concurrent" -p cache-simulator 2>/dev/null)
echo -e "  ${GREEN}✔${NC} Build complete"

# Clean if requested
if $CLEAN_MODE; then
    echo -e "  ${YELLOW}⚠${NC} Cleaning traffic data..."
    rm -rf "$SCRIPT_DIR/traffic_data"
fi

# Run benchmark
START_TIME=$(date +%s)

if $QUICK_MODE; then
    echo -e "  ${YELLOW}⚠${NC} Quick mode (~1.6M requests)"
    "$SCRIPT_DIR/run_simulations.sh" all --quick 2>&1 | tee "$BENCHMARK_DIR/output.txt"
else
    echo -e "  ${CYAN}ℹ${NC} Full mode (~33M requests, ~15 minutes)"
    "$SCRIPT_DIR/run_simulations.sh" all 2>&1 | tee "$BENCHMARK_DIR/output.txt"
fi

END_TIME=$(date +%s)
DURATION=$((END_TIME - START_TIME))

# Copy results
cp "$SCRIPT_DIR/results"/*.csv "$BENCHMARK_DIR/" 2>/dev/null || true

# Generate summary
MODE_DESC="Full (~33M requests)"
if $QUICK_MODE; then
    MODE_DESC="Quick (~1.6M requests)"
fi

cat >> "$BENCHMARK_DIR/environment.md" << EOF

## Benchmark Results

- **Mode:** $MODE_DESC
- **Duration:** ${DURATION}s

### Best Hit Rates

| Scenario | Best Algorithm | Hit Rate |
|----------|----------------|----------|
EOF

for csv in "$BENCHMARK_DIR"/*.csv; do
    [[ -f "$csv" ]] || continue
    scenario=$(basename "$csv" .csv)
    best=$(tail -n +2 "$csv" 2>/dev/null | sort -t',' -k3 -rn | head -1)
    [[ -n "$best" ]] || continue
    algo=$(echo "$best" | cut -d',' -f1)
    hit=$(echo "$best" | cut -d',' -f3)
    echo "| $scenario | $algo | $hit |" >> "$BENCHMARK_DIR/environment.md"
done

# Done
echo ""
echo -e "${BOLD}${GREEN}════════════════════════════════════════════════════════════════${NC}"
echo -e "${BOLD}${GREEN} Benchmark Complete${NC}"
echo -e "${BOLD}${GREEN}════════════════════════════════════════════════════════════════${NC}"
echo ""
echo -e "  ${GREEN}✔${NC} Duration: ${DURATION}s"
echo -e "  ${GREEN}✔${NC} Results: $BENCHMARK_DIR"
echo -e "  ${CYAN}ℹ${NC} CSV files: $(ls "$BENCHMARK_DIR"/*.csv 2>/dev/null | wc -l)"
echo ""
