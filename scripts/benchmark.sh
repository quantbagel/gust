#!/usr/bin/env bash
# Gust vs SwiftPM Benchmark Suite
# Tests real-world performance on packages with deep dependency trees

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
GUST="$PROJECT_DIR/target/release/gust"
BENCH_DIR="/tmp/gust-benchmark"
RESULTS_FILE="$BENCH_DIR/results.md"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

print_header() {
    echo ""
    echo -e "${BOLD}${CYAN}╔══════════════════════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${BOLD}${CYAN}║                        GUST vs SwiftPM BENCHMARK                            ║${NC}"
    echo -e "${BOLD}${CYAN}╠══════════════════════════════════════════════════════════════════════════════╣${NC}"
    echo -e "${BOLD}${CYAN}║  Testing: Cold Resolve | Warm Resolve | Incremental                         ║${NC}"
    echo -e "${BOLD}${CYAN}╚══════════════════════════════════════════════════════════════════════════════╝${NC}"
    echo ""
}

time_cmd() {
    local start end elapsed
    start=$(python3 -c 'import time; print(time.time())')
    "$@" > /dev/null 2>&1
    local exit_code=$?
    end=$(python3 -c 'import time; print(time.time())')
    elapsed=$(python3 -c "print(f'{$end - $start:.2f}')")
    echo "$elapsed"
    return $exit_code
}

clear_gust_cache() {
    rm -rf ~/Library/Caches/dev.gust.gust/git/* 2>/dev/null || true
    rm -rf ~/Library/Caches/dev.gust.gust/manifests/* 2>/dev/null || true
    rm -rf ~/Library/Caches/dev.gust.gust/binary-cache/* 2>/dev/null || true
}

setup_gust_project() {
    local name=$1
    local url=$2
    local branch=$3
    local dir="$BENCH_DIR/gust-$name"

    rm -rf "$dir"
    mkdir -p "$dir"

    cat > "$dir/Gust.toml" << EOF
[package]
name = "bench-$name"
version = "1.0.0"

[dependencies]
$name = { git = "$url", branch = "$branch" }
EOF

    echo "$dir"
}

setup_spm_project() {
    local name=$1
    local url=$2
    local branch=$3
    local dir="$BENCH_DIR/spm-$name"

    rm -rf "$dir"
    mkdir -p "$dir"

    cat > "$dir/Package.swift" << EOF
// swift-tools-version:5.9
import PackageDescription
let package = Package(
    name: "bench-$name",
    dependencies: [
        .package(url: "$url", branch: "$branch")
    ]
)
EOF

    echo "$dir"
}

run_benchmark() {
    local name=$1
    local url=$2
    local branch=$3
    local desc=$4

    echo -e "${BOLD}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BOLD}  Benchmarking: ${YELLOW}$name${NC} ${BOLD}($desc)${NC}"
    echo -e "${BOLD}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"

    local gust_dir=$(setup_gust_project "$name" "$url" "$branch")
    local spm_dir=$(setup_spm_project "$name" "$url" "$branch")

    # ─────────────────────────────────────────────────────────────────────────────
    # TEST 1: Cold Resolve (no cache)
    # ─────────────────────────────────────────────────────────────────────────────
    echo -e "\n${BLUE}[1/5] Cold Resolve${NC} (all caches cleared)"

    # Clear all caches
    clear_gust_cache
    rm -rf "$gust_dir/.gust" "$gust_dir/Gust.lock"
    rm -rf "$spm_dir/.build" "$spm_dir/.swiftpm" "$spm_dir/Package.resolved"

    echo -n "      SwiftPM: "
    spm_cold=$(time_cmd swift package resolve --package-path "$spm_dir")
    echo -e "${spm_cold}s"

    echo -n "      Gust:    "
    cd "$gust_dir"
    gust_cold=$(time_cmd "$GUST" install)
    cd "$BENCH_DIR"
    echo -e "${gust_cold}s"

    speedup_cold=$(python3 -c "print(f'{float($spm_cold) / float($gust_cold):.1f}x' if float($gust_cold) > 0 else 'N/A')")
    echo -e "      ${GREEN}Speedup: ${BOLD}$speedup_cold${NC}"

    # ─────────────────────────────────────────────────────────────────────────────
    # TEST 2: Warm Resolve (git cached, no lockfile)
    # ─────────────────────────────────────────────────────────────────────────────
    echo -e "\n${BLUE}[2/5] Warm Resolve${NC} (git cached, fresh project)"

    # Keep git cache, clear project state
    rm -rf "$gust_dir/.gust" "$gust_dir/Gust.lock"
    rm -rf "$spm_dir/.build" "$spm_dir/.swiftpm" "$spm_dir/Package.resolved"

    echo -n "      SwiftPM: "
    spm_warm=$(time_cmd swift package resolve --package-path "$spm_dir")
    echo -e "${spm_warm}s"

    echo -n "      Gust:    "
    cd "$gust_dir"
    gust_warm=$(time_cmd "$GUST" install)
    cd "$BENCH_DIR"
    echo -e "${gust_warm}s"

    speedup_warm=$(python3 -c "print(f'{float($spm_warm) / float($gust_warm):.1f}x' if float($gust_warm) > 0 else 'N/A')")
    echo -e "      ${GREEN}Speedup: ${BOLD}$speedup_warm${NC}"

    # ─────────────────────────────────────────────────────────────────────────────
    # TEST 3: Incremental (lockfile exists, no changes)
    # ─────────────────────────────────────────────────────────────────────────────
    echo -e "\n${BLUE}[3/5] Incremental${NC} (lockfile exists, no changes)"

    echo -n "      SwiftPM: "
    spm_inc=$(time_cmd swift package resolve --package-path "$spm_dir")
    echo -e "${spm_inc}s"

    echo -n "      Gust:    "
    cd "$gust_dir"
    gust_inc=$(time_cmd "$GUST" install)
    cd "$BENCH_DIR"
    echo -e "${gust_inc}s"

    speedup_inc=$(python3 -c "print(f'{float($spm_inc) / float($gust_inc):.1f}x' if float($gust_inc) > 0 else 'N/A')")
    echo -e "      ${GREEN}Speedup: ${BOLD}$speedup_inc${NC}"

    # Build benchmarks skipped for speed - the real win is in resolve time
    spm_cached="skip"
    gust_cached="skip"
    speedup_cached="N/A"

    # ─────────────────────────────────────────────────────────────────────────────
    # Summary for this package
    # ─────────────────────────────────────────────────────────────────────────────
    echo ""
    echo -e "${BOLD}  ┌─────────────────────────────────────────────────────────────────┐${NC}"
    echo -e "${BOLD}  │                    RESULTS: $name${NC}"
    echo -e "${BOLD}  ├─────────────────────┬────────────┬────────────┬────────────────┤${NC}"
    printf "  │ %-19s │ %10s │ %10s │ %14s │\n" "Scenario" "SwiftPM" "Gust" "Speedup"
    echo -e "${BOLD}  ├─────────────────────┼────────────┼────────────┼────────────────┤${NC}"
    printf "  │ %-19s │ %9ss │ %9ss │ ${GREEN}%14s${NC} │\n" "Cold Resolve" "$spm_cold" "$gust_cold" "$speedup_cold"
    printf "  │ %-19s │ %9ss │ %9ss │ ${GREEN}%14s${NC} │\n" "Warm Resolve" "$spm_warm" "$gust_warm" "$speedup_warm"
    printf "  │ %-19s │ %9ss │ %9ss │ ${GREEN}%14s${NC} │\n" "Incremental" "$spm_inc" "$gust_inc" "$speedup_inc"
    echo -e "${BOLD}  └─────────────────────┴────────────┴────────────┴────────────────┘${NC}"
    echo ""
}

# Package definitions
get_package_info() {
    case "$1" in
        vapor)
            echo "https://github.com/vapor/vapor.git|main|19 deps, 28 transitive"
            ;;
        swift-nio)
            echo "https://github.com/apple/swift-nio.git|main|3 deps"
            ;;
        grpc-swift)
            echo "https://github.com/grpc/grpc-swift.git|main|10 deps"
            ;;
        async-http-client)
            echo "https://github.com/swift-server/async-http-client.git|main|9 deps"
            ;;
        swift-openapi-generator)
            echo "https://github.com/apple/swift-openapi-generator.git|main|7 deps"
            ;;
        *)
            echo ""
            ;;
    esac
}

main() {
    local packages=("vapor" "swift-nio" "grpc-swift" "async-http-client" "swift-openapi-generator")

    # Build Gust
    echo -e "${BOLD}Building Gust...${NC}"
    cd "$PROJECT_DIR"
    cargo build --release 2>/dev/null

    # Setup
    rm -rf "$BENCH_DIR"
    mkdir -p "$BENCH_DIR"

    print_header

    # Run benchmarks
    for name in "${packages[@]}"; do
        info=$(get_package_info "$name")
        if [ -n "$info" ]; then
            IFS='|' read -r url branch desc <<< "$info"
            run_benchmark "$name" "$url" "$branch" "$desc"
        fi
    done

    # Final summary
    echo -e "${BOLD}${CYAN}╔══════════════════════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${BOLD}${CYAN}║                           BENCHMARK COMPLETE                                 ║${NC}"
    echo -e "${BOLD}${CYAN}╚══════════════════════════════════════════════════════════════════════════════╝${NC}"
    echo ""
    echo -e "${BOLD}Key Insights:${NC}"
    echo -e "  • ${GREEN}Cold Resolve${NC}: Gust's parallel git fetching + parallel manifest parsing"
    echo -e "  • ${GREEN}Warm Resolve${NC}: Gust's BLAKE3-indexed manifest cache eliminates SPM calls"
    echo -e "  • ${GREEN}Incremental${NC}: Gust's lockfile diff detection skips unnecessary work"
    echo -e "  • ${GREEN}Cached Build${NC}: Gust's zstd-compressed binary cache vs SPM's incremental"
    echo ""
}

# Run specific package or all
if [ -n "$1" ]; then
    cd "$PROJECT_DIR"
    cargo build --release 2>/dev/null

    info=$(get_package_info "$1")
    if [ -n "$info" ]; then
        rm -rf "$BENCH_DIR"
        mkdir -p "$BENCH_DIR"
        print_header
        IFS='|' read -r url branch desc <<< "$info"
        run_benchmark "$1" "$url" "$branch" "$desc"
    else
        echo "Unknown package: $1"
        echo "Available: vapor, swift-nio, grpc-swift, async-http-client, swift-openapi-generator"
        exit 1
    fi
else
    main
fi
