#!/bin/bash
#
# Cache Algorithm Simulation Suite
# ================================
# A comprehensive tool for generating traffic data and benchmarking
# cache eviction algorithms (LRU, SLRU, LFU, LFUDA, GDSF, Moka).
#
# Usage:
#   ./run_simulations.sh [command] [options]
#
# Commands:
#   generate    Generate traffic data for simulations
#   simulate    Run cache simulations on generated traffic
#   all         Generate data and run all simulations (default)
#   clean       Remove generated traffic and results
#   help        Show this help message
#
# Examples:
#   ./run_simulations.sh                    # Run everything with defaults
#   ./run_simulations.sh generate           # Only generate traffic data
#   ./run_simulations.sh simulate           # Only run simulations (data must exist)
#   ./run_simulations.sh all --quick        # Quick run with fewer iterations
#   ./run_simulations.sh clean              # Clean up generated files
#

set -e

# =============================================================================
# Configuration
# =============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SIMULATOR="$REPO_ROOT/target/release/cache-simulator"

# Output directories
TRAFFIC_DIR="$SCRIPT_DIR/traffic_data"
RESULTS_DIR="$SCRIPT_DIR/results"

# Colors for pretty output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
MAGENTA='\033[0;35m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m' # No Color

# Default settings
QUICK_MODE=false
VERBOSE=false

# =============================================================================
# Traffic Generation Profiles
# =============================================================================
# Each profile simulates a different real-world caching scenario.
# Format: name|rps|duration|objects|popular_traffic|popular_objects|min_size|max_size|description

declare -a TRAFFIC_PROFILES=(
    # Video streaming: Large objects, concentrated popularity (viral videos)
    "video|500|4|10000|70|10|1024|10240|Video streaming CDN - large files, viral content"
    
    # Social media: Small objects, highly skewed (profile pics, thumbnails)
    "social|1000|4|100000|90|5|10|100|Social media - small objects, extreme skew"
    
    # Web/API: Mixed sizes, moderate skew (typical web caching)
    "web|800|4|50000|60|20|100|5120|Web/API gateway - mixed sizes, moderate popularity"
)

# Quick mode uses smaller datasets
declare -a TRAFFIC_PROFILES_QUICK=(
    "video|100|1|1000|70|10|1024|10240|Video (quick)"
    "social|200|1|10000|90|5|10|100|Social (quick)"
    "web|150|1|5000|60|20|100|5120|Web (quick)"
)

# =============================================================================
# Simulation Configurations
# =============================================================================

# Capacity-constrained tests (entry count limits)
declare -a CAPACITIES=(500 2500 5000)
declare -a CAPACITIES_QUICK=(500 2500)

# Size-constrained tests (byte limits) - in MB
declare -a MAX_SIZES_MB=(50 250 500)
declare -a MAX_SIZES_MB_QUICK=(50 250)

# =============================================================================
# Helper Functions
# =============================================================================

print_header() {
    echo ""
    echo -e "${BOLD}${BLUE}╔════════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${BOLD}${BLUE}║${NC} ${BOLD}$1${NC}"
    echo -e "${BOLD}${BLUE}╚════════════════════════════════════════════════════════════════╝${NC}"
    echo ""
}

print_section() {
    echo ""
    echo -e "${CYAN}┌────────────────────────────────────────────────────────────────┐${NC}"
    echo -e "${CYAN}│${NC} ${BOLD}$1${NC}"
    echo -e "${CYAN}└────────────────────────────────────────────────────────────────┘${NC}"
}

print_step() {
    echo -e "  ${GREEN}▸${NC} $1"
}

print_substep() {
    echo -e "    ${YELLOW}→${NC} $1"
}

# Table formatting with fixed-width columns
TABLE_FMT="%-6s %-12s %8s %8s %10s %8s %12s %8s %8s %8s"

print_table_header() {
    local header
    header=$(printf "$TABLE_FMT" "Algo" "Mode" "Hit%" "Byte%" "Evictions" "Time" "Throughput" "GET" "PUT" "p99")
    local width=${#header}
    local border=$(printf '─%.0s' $(seq 1 $((width + 2))))
    
    echo -e "    ${CYAN}┌${border}┐${NC}"
    echo -e "    ${CYAN}│${NC} ${BOLD}${header}${NC} ${CYAN}│${NC}"
    echo -e "    ${CYAN}├${border}┤${NC}"
}

print_table_footer() {
    local header
    header=$(printf "$TABLE_FMT" "Algo" "Mode" "Hit%" "Byte%" "Evictions" "Time" "Throughput" "GET" "PUT" "p99")
    local width=${#header}
    local border=$(printf '─%.0s' $(seq 1 $((width + 2))))
    
    echo -e "    ${CYAN}└${border}┘${NC}"
}

print_table_row() {
    # Parse the input line and reformat with fixed widths
    # Input format: "LRU    Sequential   73.26%     73.19%       912503     0.096s    9466153        79ns       203ns       110ns"
    local line="$1"
    local formatted
    formatted=$(echo "$line" | awk '{printf "%-6s %-12s %8s %8s %10s %8s %12s %8s %8s %8s", $1, $2, $3, $4, $5, $6, $7, $8, $9, $10}')
    echo -e "    ${CYAN}│${NC} ${formatted} ${CYAN}│${NC}"
}

print_success() {
    echo -e "  ${GREEN}✔${NC} $1"
}

print_error() {
    echo -e "  ${RED}✖${NC} $1" >&2
}

print_warning() {
    echo -e "  ${YELLOW}⚠${NC} $1"
}

print_info() {
    echo -e "  ${BLUE}ℹ${NC} $1"
}

# Format duration in human-readable form
format_duration() {
    local seconds=$1
    if (( seconds < 60 )); then
        echo "${seconds}s"
    elif (( seconds < 3600 )); then
        echo "$((seconds / 60))m $((seconds % 60))s"
    else
        echo "$((seconds / 3600))h $((seconds % 3600 / 60))m"
    fi
}

# =============================================================================
# Build Functions
# =============================================================================

ensure_built() {
    print_section "Checking Build"
    
    if [[ ! -f "$SIMULATOR" ]]; then
        print_step "Building cache-simulator (release mode)..."
        cd "$REPO_ROOT"
        if [[ "$VERBOSE" == "true" ]]; then
            cargo build --release -p cache-simulator
        else
            cargo build --release -p cache-simulator 2>&1 | tail -5
        fi
        print_success "Build complete"
    else
        print_success "cache-simulator already built"
    fi
    
    # Verify it works
    if ! "$SIMULATOR" --version &>/dev/null; then
        print_error "Failed to run cache-simulator"
        exit 1
    fi
}

# =============================================================================
# Traffic Generation
# =============================================================================

generate_traffic() {
    local profiles
    if [[ "$QUICK_MODE" == "true" ]]; then
        profiles=("${TRAFFIC_PROFILES_QUICK[@]}")
    else
        profiles=("${TRAFFIC_PROFILES[@]}")
    fi
    
    print_section "Generating Traffic Data"
    print_info "Output directory: $TRAFFIC_DIR"
    echo ""
    
    mkdir -p "$TRAFFIC_DIR"
    
    local total=${#profiles[@]}
    local current=0
    
    for profile in "${profiles[@]}"; do
        IFS='|' read -r name rps duration objects pop_traffic pop_objects min_size max_size desc <<< "$profile"
        current=$((current + 1))
        
        local output_dir="$TRAFFIC_DIR/${name}_traffic"
        
        print_step "[$current/$total] Generating: ${BOLD}$name${NC}"
        print_substep "$desc"
        print_substep "RPS=$rps, Duration=${duration}h, Objects=$objects"
        print_substep "Popularity: ${pop_traffic}% traffic → ${pop_objects}% objects"
        print_substep "Size: ${min_size}KB - ${max_size}KB"
        
        # Skip if already exists and not empty
        if [[ -d "$output_dir" ]] && [[ -n "$(ls -A "$output_dir" 2>/dev/null)" ]]; then
            print_warning "Already exists, skipping (use 'clean' to regenerate)"
            continue
        fi
        
        local start_time=$SECONDS
        
        "$SIMULATOR" generate \
            --rps "$rps" \
            --duration "$duration" \
            --objects "$objects" \
            --popular-traffic "$pop_traffic" \
            --popular-objects "$pop_objects" \
            --min-size "$min_size" \
            --max-size "$max_size" \
            --output "$output_dir" \
            2>&1 | if [[ "$VERBOSE" == "true" ]]; then cat; else grep -E "^(Generating|Hour|complete)" || true; fi
        
        local elapsed=$((SECONDS - start_time))
        print_success "Generated in $(format_duration $elapsed)"
        echo ""
    done
}

# =============================================================================
# Simulation Runner
# =============================================================================

run_simulations() {
    local capacities max_sizes_mb
    if [[ "$QUICK_MODE" == "true" ]]; then
        capacities=("${CAPACITIES_QUICK[@]}")
        max_sizes_mb=("${MAX_SIZES_MB_QUICK[@]}")
    else
        capacities=("${CAPACITIES[@]}")
        max_sizes_mb=("${MAX_SIZES_MB[@]}")
    fi
    
    print_section "Running Cache Simulations"
    print_info "Results directory: $RESULTS_DIR"
    echo ""
    
    mkdir -p "$RESULTS_DIR"
    
    # Discover available traffic patterns
    local traffic_dirs=()
    for dir in "$TRAFFIC_DIR"/*_traffic; do
        if [[ -d "$dir" ]]; then
            traffic_dirs+=("$dir")
        fi
    done
    
    if [[ ${#traffic_dirs[@]} -eq 0 ]]; then
        print_error "No traffic data found in $TRAFFIC_DIR"
        print_info "Run './run_simulations.sh generate' first"
        exit 1
    fi
    
    print_info "Found ${#traffic_dirs[@]} traffic pattern(s)"
    for dir in "${traffic_dirs[@]}"; do
        print_substep "$(basename "$dir")"
    done
    echo ""
    
    # =========================================================================
    # Part 1: Capacity-Constrained Simulations
    # =========================================================================
    print_section "Part 1: Capacity-Constrained Mode"
    print_info "Cache evicts based on number of entries"
    echo ""
    
    for traffic_dir in "${traffic_dirs[@]}"; do
        local traffic_name=$(basename "$traffic_dir" | sed 's/_traffic$//')
        
        echo -e "  ${MAGENTA}━━━ Traffic: ${BOLD}$traffic_name${NC} ${MAGENTA}━━━${NC}"
        
        for capacity in "${capacities[@]}"; do
            local output_file="$RESULTS_DIR/${traffic_name}_capacity_${capacity}.csv"
            
            print_step "Capacity: $capacity entries"
            print_table_header
            
            local start_time=$SECONDS
            
            "$SIMULATOR" simulate \
                --input-dir "$traffic_dir" \
                --capacity "$capacity" \
                --mode both \
                --output-csv "$output_file" \
                2>&1 | grep -E "^\s*(LRU|SLRU|LFU|LFUDA|GDSF|Moka)" | head -12 | while read -r line; do
                    print_table_row "$line"
                done
            
            print_table_footer
            
            local elapsed=$((SECONDS - start_time))
            print_success "Completed in $(format_duration $elapsed) → $output_file"
        done
        echo ""
    done
    
    # =========================================================================
    # Part 2: Size-Constrained Simulations
    # =========================================================================
    print_section "Part 2: Size-Constrained Mode"
    print_info "Cache evicts based on total byte storage (--use-size)"
    echo ""
    
    for traffic_dir in "${traffic_dirs[@]}"; do
        local traffic_name=$(basename "$traffic_dir" | sed 's/_traffic$//')
        
        echo -e "  ${MAGENTA}━━━ Traffic: ${BOLD}$traffic_name${NC} ${MAGENTA}━━━${NC}"
        
        for size_mb in "${max_sizes_mb[@]}"; do
            local max_size=$((size_mb * 1048576))
            local output_file="$RESULTS_DIR/${traffic_name}_size_${size_mb}mb.csv"
            
            print_step "Max size: ${size_mb}MB"
            print_table_header
            
            local start_time=$SECONDS
            
            "$SIMULATOR" simulate \
                --input-dir "$traffic_dir" \
                --capacity 1000000 \
                --max-size "$max_size" \
                --use-size \
                --mode both \
                --output-csv "$output_file" \
                2>&1 | grep -E "^\s*(LRU|SLRU|LFU|LFUDA|GDSF|Moka)" | head -12 | while read -r line; do
                    print_table_row "$line"
                done
            
            print_table_footer
            local elapsed=$((SECONDS - start_time))
            print_success "Completed in $(format_duration $elapsed) → $output_file"
        done
        echo ""
    done
}

# =============================================================================
# Results Summary
# =============================================================================

print_results_summary() {
    print_section "Results Summary"
    
    if [[ ! -d "$RESULTS_DIR" ]] || [[ -z "$(ls -A "$RESULTS_DIR" 2>/dev/null)" ]]; then
        print_warning "No results found"
        return
    fi
    
    local csv_count=$(find "$RESULTS_DIR" -name "*.csv" | wc -l)
    local total_size=$(du -sh "$RESULTS_DIR" 2>/dev/null | cut -f1)
    
    print_info "Generated $csv_count result files ($total_size)"
    echo ""
    
    # Quick summary table from CSV files
    echo -e "  ${BOLD}Best Hit Rates by Scenario:${NC}"
    echo ""
    printf "  %-25s %-12s %-10s\n" "Scenario" "Best Algo" "Hit Rate"
    echo "  ─────────────────────────────────────────────────"
    
    for csv_file in "$RESULTS_DIR"/*.csv; do
        if [[ -f "$csv_file" ]]; then
            local scenario=$(basename "$csv_file" .csv)
            # Extract best sequential algorithm (skip header, sort by hit_rate, take first)
            local best=$(tail -n +2 "$csv_file" 2>/dev/null | \
                         grep "Sequential" | \
                         sort -t',' -k5 -rn | \
                         head -1 | \
                         awk -F',' '{printf "%-12s %.2f%%", $1, $5}')
            if [[ -n "$best" ]]; then
                printf "  %-25s %s\n" "$scenario" "$best"
            fi
        fi
    done
    echo ""
}

# =============================================================================
# Cleanup
# =============================================================================

clean_all() {
    print_section "Cleaning Generated Files"
    
    if [[ -d "$TRAFFIC_DIR" ]]; then
        print_step "Removing traffic data: $TRAFFIC_DIR"
        rm -rf "$TRAFFIC_DIR"
        print_success "Removed"
    else
        print_info "No traffic data to remove"
    fi
    
    if [[ -d "$RESULTS_DIR" ]]; then
        print_step "Removing results: $RESULTS_DIR"
        rm -rf "$RESULTS_DIR"
        print_success "Removed"
    else
        print_info "No results to remove"
    fi
    
    print_success "Cleanup complete"
}

# =============================================================================
# Help
# =============================================================================

show_help() {
    cat << 'EOF'

  ╔═══════════════════════════════════════════════════════════════════╗
  ║           Cache Algorithm Simulation Suite                        ║
  ╚═══════════════════════════════════════════════════════════════════╝

  USAGE:
      ./run_simulations.sh [command] [options]

  COMMANDS:
      all         Generate traffic data and run all simulations (default)
      generate    Generate traffic data only
      simulate    Run simulations only (traffic data must exist)
      clean       Remove all generated traffic and results
      help        Show this help message

  OPTIONS:
      --quick     Use smaller datasets for faster iteration
      --verbose   Show detailed output from subcommands
      --help      Show this help message

  EXAMPLES:
      # Full run with all traffic patterns and cache sizes
      ./run_simulations.sh

      # Quick run for testing/development
      ./run_simulations.sh --quick

      # Only generate traffic data
      ./run_simulations.sh generate

      # Only run simulations (after generating data)
      ./run_simulations.sh simulate

      # Clean up and start fresh
      ./run_simulations.sh clean
      ./run_simulations.sh all

  TRAFFIC PATTERNS:
      video     Large objects (1-10MB), concentrated popularity
                Simulates video streaming CDN with viral content

      social    Small objects (10-100KB), extreme popularity skew
                Simulates social media with profile pics, thumbnails

      web       Mixed sizes (100KB-5MB), moderate popularity
                Simulates typical web/API gateway caching

  ALGORITHMS TESTED:
      LRU       Least Recently Used - simple recency-based
      SLRU      Segmented LRU - scan-resistant, two-tier
      LFU       Least Frequently Used - frequency-based
      LFUDA     LFU with Dynamic Aging - handles shifting popularity
      GDSF      Greedy Dual Size Frequency - size-aware
      Moka      External high-performance cache (comparison baseline)

  OUTPUT:
      Traffic data:  ./traffic_data/<pattern>_traffic/
      Results:       ./results/<pattern>_<config>.csv

  For more details, see the cache-simulator README.md

EOF
}

# =============================================================================
# Main
# =============================================================================

main() {
    local command="all"
    
    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case $1 in
            generate|simulate|all|clean|help)
                command="$1"
                shift
                ;;
            --quick|-q)
                QUICK_MODE=true
                shift
                ;;
            --verbose|-v)
                VERBOSE=true
                shift
                ;;
            --help|-h)
                show_help
                exit 0
                ;;
            *)
                print_error "Unknown option: $1"
                echo "Run './run_simulations.sh help' for usage"
                exit 1
                ;;
        esac
    done
    
    # Header
    print_header "Cache Algorithm Simulation Suite"
    
    if [[ "$QUICK_MODE" == "true" ]]; then
        print_warning "Quick mode enabled (smaller datasets)"
    fi
    
    local start_time=$SECONDS
    
    # Execute command
    case $command in
        generate)
            ensure_built
            generate_traffic
            ;;
        simulate)
            ensure_built
            run_simulations
            print_results_summary
            ;;
        all)
            ensure_built
            generate_traffic
            run_simulations
            print_results_summary
            ;;
        clean)
            clean_all
            ;;
        help)
            show_help
            exit 0
            ;;
    esac
    
    # Footer
    local total_elapsed=$((SECONDS - start_time))
    echo ""
    print_header "Complete!"
    print_success "Total time: $(format_duration $total_elapsed)"
    echo ""
}

# Run main
main "$@"
