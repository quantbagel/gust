#!/usr/bin/env bash
# Gust vs SwiftPM Benchmark Suite
# Tests real-world performance on packages with deep dependency trees
# Runs all tests in parallel for speed

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
GUST="$PROJECT_DIR/target/release/gust"
BENCH_DIR="/tmp/gust-benchmark"

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
    echo -e "${BOLD}${CYAN}║  Cold Resolve | Warm Resolve | Incremental | Cold Build | Cached Build      ║${NC}"
    echo -e "${BOLD}${CYAN}╚══════════════════════════════════════════════════════════════════════════════╝${NC}"
    echo ""
}

time_cmd() {
    local start end elapsed
    start=$(python3 -c 'import time; print(time.time())')
    "$@" > /dev/null 2>&1 || true
    end=$(python3 -c 'import time; print(time.time())')
    elapsed=$(python3 -c "print(f'{$end - $start:.2f}')")
    echo "$elapsed"
}

clear_gust_cache() {
    rm -rf ~/Library/Caches/dev.gust.gust/git/* 2>/dev/null || true
    rm -rf ~/Library/Caches/dev.gust.gust/manifests/* 2>/dev/null || true
    rm -rf ~/Library/Caches/dev.gust.gust/binary-cache/* 2>/dev/null || true
}

setup_project() {
    local name=$1
    local url=$2
    local branch=$3
    local type=$4  # "gust" or "spm"
    local dir="$BENCH_DIR/${type}-${name}"

    rm -rf "$dir"
    mkdir -p "$dir/Sources/App"

    if [ "$type" = "gust" ]; then
        cat > "$dir/Gust.toml" << EOF
[package]
name = "bench-$name"
version = "1.0.0"

[dependencies]
$name = { git = "$url", branch = "$branch" }
EOF
    else
        cat > "$dir/Package.swift" << EOF
// swift-tools-version:5.9
import PackageDescription
let package = Package(
    name: "bench-$name",
    platforms: [.macOS(.v13)],
    dependencies: [
        .package(url: "$url", branch: "$branch")
    ],
    targets: [
        .executableTarget(name: "App", dependencies: [.product(name: "Vapor", package: "$name")])
    ]
)
EOF
    fi

    # Add source file for builds
    echo 'print("Hello")' > "$dir/Sources/App/main.swift"

    echo "$dir"
}

# Run a single benchmark scenario and save result to file
run_scenario() {
    local scenario=$1
    local tool=$2      # "spm" or "gust"
    local name=$3
    local url=$4
    local branch=$5
    local result_file=$6

    local dir="$BENCH_DIR/${tool}-${name}-${scenario}"

    case "$scenario" in
        cold)
            # Cold resolve - fresh everything
            rm -rf "$dir"
            mkdir -p "$dir"

            if [ "$tool" = "gust" ]; then
                cat > "$dir/Gust.toml" << EOF
[package]
name = "bench-$name"
version = "1.0.0"

[dependencies]
$name = { git = "$url", branch = "$branch" }
EOF
                # Clear gust cache for cold test
                rm -rf ~/Library/Caches/dev.gust.gust/git/* 2>/dev/null || true
                rm -rf ~/Library/Caches/dev.gust.gust/manifests/* 2>/dev/null || true

                cd "$dir"
                time_cmd "$GUST" install > "$result_file"
            else
                cat > "$dir/Package.swift" << EOF
// swift-tools-version:5.9
import PackageDescription
let package = Package(
    name: "bench-$name",
    dependencies: [.package(url: "$url", branch: "$branch")]
)
EOF
                rm -rf "$dir/.build" "$dir/.swiftpm"
                time_cmd swift package resolve --package-path "$dir" > "$result_file"
            fi
            ;;

        warm)
            # Warm resolve - git cached, no lockfile
            dir="$BENCH_DIR/${tool}-${name}-warm"
            rm -rf "$dir"
            mkdir -p "$dir"

            if [ "$tool" = "gust" ]; then
                cat > "$dir/Gust.toml" << EOF
[package]
name = "bench-$name"
version = "1.0.0"

[dependencies]
$name = { git = "$url", branch = "$branch" }
EOF
                cd "$dir"
                time_cmd "$GUST" install > "$result_file"
            else
                cat > "$dir/Package.swift" << EOF
// swift-tools-version:5.9
import PackageDescription
let package = Package(
    name: "bench-$name",
    dependencies: [.package(url: "$url", branch: "$branch")]
)
EOF
                time_cmd swift package resolve --package-path "$dir" > "$result_file"
            fi
            ;;

        incremental)
            # Incremental - lockfile exists, run again
            dir="$BENCH_DIR/${tool}-${name}-warm"  # Reuse warm dir which has lockfile

            if [ "$tool" = "gust" ]; then
                cd "$dir"
                time_cmd "$GUST" install > "$result_file"
            else
                time_cmd swift package resolve --package-path "$dir" > "$result_file"
            fi
            ;;

        cold_build)
            # Cold build - no binary cache
            dir="$BENCH_DIR/${tool}-${name}-build"
            rm -rf "$dir"
            mkdir -p "$dir/Sources/App"

            if [ "$tool" = "gust" ]; then
                rm -rf ~/Library/Caches/dev.gust.gust/binary-cache/* 2>/dev/null || true
                cat > "$dir/Gust.toml" << EOF
[package]
name = "bench-$name"
version = "1.0.0"

[dependencies]
$name = { git = "$url", branch = "$branch" }
EOF
                cat > "$dir/Sources/App/main.swift" << 'EOF'
import Vapor
let app = Application()
print("Hello")
EOF
                cd "$dir"
                "$GUST" install > /dev/null 2>&1 || true
                time_cmd "$GUST" build > "$result_file"
            else
                cat > "$dir/Package.swift" << EOF
// swift-tools-version:5.9
import PackageDescription
let package = Package(
    name: "bench-$name",
    platforms: [.macOS(.v13)],
    dependencies: [.package(url: "$url", branch: "$branch")],
    targets: [.executableTarget(name: "App", dependencies: [.product(name: "Vapor", package: "$name")])]
)
EOF
                cat > "$dir/Sources/App/main.swift" << 'EOF'
import Vapor
let app = Application()
print("Hello")
EOF
                swift package resolve --package-path "$dir" > /dev/null 2>&1 || true
                time_cmd swift build --package-path "$dir" > "$result_file"
            fi
            ;;

        cached_build)
            # Cached build - binary cache populated
            dir="$BENCH_DIR/${tool}-${name}-build"

            if [ "$tool" = "gust" ]; then
                # Clear .build but keep binary cache
                rm -rf "$dir/.build"
                cd "$dir"
                time_cmd "$GUST" build > "$result_file"
            else
                # SPM incremental (no changes)
                time_cmd swift build --package-path "$dir" > "$result_file"
            fi
            ;;
    esac
}

run_benchmark() {
    local name=$1
    local url=$2
    local branch=$3
    local desc=$4

    echo -e "${BOLD}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BOLD}  Benchmarking: ${YELLOW}$name${NC} ${BOLD}($desc)${NC}"
    echo -e "${BOLD}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo ""

    local results_dir="$BENCH_DIR/results-$name"
    mkdir -p "$results_dir"

    # Clear caches before cold tests
    clear_gust_cache

    echo -e "  ${BLUE}Running all scenarios in parallel...${NC}"
    echo ""

    # Run cold tests first (they need clean cache)
    echo -n "  [1/5] Cold Resolve:   "
    run_scenario cold spm "$name" "$url" "$branch" "$results_dir/spm_cold" &
    local pid_spm_cold=$!
    run_scenario cold gust "$name" "$url" "$branch" "$results_dir/gust_cold" &
    local pid_gust_cold=$!
    wait $pid_spm_cold $pid_gust_cold
    local spm_cold=$(cat "$results_dir/spm_cold")
    local gust_cold=$(cat "$results_dir/gust_cold")
    local speedup_cold=$(python3 -c "print(f'{float($spm_cold) / float($gust_cold):.1f}x' if float($gust_cold) > 0.001 else 'N/A')")
    echo -e "SPM ${spm_cold}s | Gust ${gust_cold}s | ${GREEN}${speedup_cold}${NC}"

    # Run warm tests (cache populated from cold)
    echo -n "  [2/5] Warm Resolve:   "
    run_scenario warm spm "$name" "$url" "$branch" "$results_dir/spm_warm" &
    local pid_spm_warm=$!
    run_scenario warm gust "$name" "$url" "$branch" "$results_dir/gust_warm" &
    local pid_gust_warm=$!
    wait $pid_spm_warm $pid_gust_warm
    local spm_warm=$(cat "$results_dir/spm_warm")
    local gust_warm=$(cat "$results_dir/gust_warm")
    local speedup_warm=$(python3 -c "print(f'{float($spm_warm) / float($gust_warm):.1f}x' if float($gust_warm) > 0.001 else 'N/A')")
    echo -e "SPM ${spm_warm}s | Gust ${gust_warm}s | ${GREEN}${speedup_warm}${NC}"

    # Run incremental tests
    echo -n "  [3/5] Incremental:    "
    run_scenario incremental spm "$name" "$url" "$branch" "$results_dir/spm_inc" &
    local pid_spm_inc=$!
    run_scenario incremental gust "$name" "$url" "$branch" "$results_dir/gust_inc" &
    local pid_gust_inc=$!
    wait $pid_spm_inc $pid_gust_inc
    local spm_inc=$(cat "$results_dir/spm_inc")
    local gust_inc=$(cat "$results_dir/gust_inc")
    local speedup_inc=$(python3 -c "print(f'{float($spm_inc) / float($gust_inc):.1f}x' if float($gust_inc) > 0.001 else 'N/A')")
    echo -e "SPM ${spm_inc}s | Gust ${gust_inc}s | ${GREEN}${speedup_inc}${NC}"

    # Run cold build (sequential - both use swift build)
    echo -n "  [4/5] Cold Build:     "
    run_scenario cold_build spm "$name" "$url" "$branch" "$results_dir/spm_cold_build"
    local spm_cold_build=$(cat "$results_dir/spm_cold_build")
    run_scenario cold_build gust "$name" "$url" "$branch" "$results_dir/gust_cold_build"
    local gust_cold_build=$(cat "$results_dir/gust_cold_build")
    local speedup_cold_build=$(python3 -c "print(f'{float($spm_cold_build) / float($gust_cold_build):.1f}x' if float($gust_cold_build) > 0.001 else 'N/A')")
    echo -e "SPM ${spm_cold_build}s | Gust ${gust_cold_build}s | ${GREEN}${speedup_cold_build}${NC}"

    # Run cached build
    echo -n "  [5/5] Cached Build:   "
    run_scenario cached_build spm "$name" "$url" "$branch" "$results_dir/spm_cached" &
    local pid_spm_cached=$!
    run_scenario cached_build gust "$name" "$url" "$branch" "$results_dir/gust_cached" &
    local pid_gust_cached=$!
    wait $pid_spm_cached $pid_gust_cached
    local spm_cached=$(cat "$results_dir/spm_cached")
    local gust_cached=$(cat "$results_dir/gust_cached")
    local speedup_cached=$(python3 -c "print(f'{float($spm_cached) / float($gust_cached):.1f}x' if float($gust_cached) > 0.001 else 'N/A')")
    echo -e "SPM ${spm_cached}s | Gust ${gust_cached}s | ${GREEN}${speedup_cached}${NC}"

    # Summary table
    echo ""
    echo -e "${BOLD}  ┌───────────────────┬────────────┬────────────┬──────────┐${NC}"
    echo -e "${BOLD}  │     $name${NC}"
    echo -e "${BOLD}  ├───────────────────┼────────────┼────────────┼──────────┤${NC}"
    printf "  │ %-17s │ %10s │ %10s │ %8s │\n" "Scenario" "SwiftPM" "Gust" "Speedup"
    echo -e "${BOLD}  ├───────────────────┼────────────┼────────────┼──────────┤${NC}"
    printf "  │ %-17s │ %9ss │ %9ss │ ${GREEN}%8s${NC} │\n" "Cold Resolve" "$spm_cold" "$gust_cold" "$speedup_cold"
    printf "  │ %-17s │ %9ss │ %9ss │ ${GREEN}%8s${NC} │\n" "Warm Resolve" "$spm_warm" "$gust_warm" "$speedup_warm"
    printf "  │ %-17s │ %9ss │ %9ss │ ${GREEN}%8s${NC} │\n" "Incremental" "$spm_inc" "$gust_inc" "$speedup_inc"
    printf "  │ %-17s │ %9ss │ %9ss │ ${GREEN}%8s${NC} │\n" "Cold Build" "$spm_cold_build" "$gust_cold_build" "$speedup_cold_build"
    printf "  │ %-17s │ %9ss │ %9ss │ ${GREEN}%8s${NC} │\n" "Cached Build" "$spm_cached" "$gust_cached" "$speedup_cached"
    echo -e "${BOLD}  └───────────────────┴────────────┴────────────┴──────────┘${NC}"
    echo ""
}

# Package definitions
get_package_info() {
    case "$1" in
        vapor)
            echo "https://github.com/vapor/vapor.git|main|19 deps, 28 transitive"
            ;;
        *)
            echo ""
            ;;
    esac
}

main() {
    # Build Gust
    echo -e "${BOLD}Building Gust...${NC}"
    cd "$PROJECT_DIR"
    cargo build --release 2>/dev/null

    # Setup
    rm -rf "$BENCH_DIR"
    mkdir -p "$BENCH_DIR"

    print_header

    # Just run vapor for now - it's the most representative
    run_benchmark "vapor" "https://github.com/vapor/vapor.git" "main" "19 deps, 28 transitive"

    # Final summary
    echo -e "${BOLD}${CYAN}╔══════════════════════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${BOLD}${CYAN}║                           BENCHMARK COMPLETE                                 ║${NC}"
    echo -e "${BOLD}${CYAN}╚══════════════════════════════════════════════════════════════════════════════╝${NC}"
    echo ""
    echo -e "${BOLD}Key Insights:${NC}"
    echo -e "  • ${GREEN}Cold Resolve${NC}:  Parallel git fetching + parallel manifest parsing"
    echo -e "  • ${GREEN}Warm Resolve${NC}:  BLAKE3-indexed manifest cache skips all SPM calls"
    echo -e "  • ${GREEN}Incremental${NC}:   Lockfile diff detection skips unnecessary work"
    echo -e "  • ${GREEN}Cold Build${NC}:    Both use swift build (similar times expected)"
    echo -e "  • ${GREEN}Cached Build${NC}:  Gust's zstd binary cache vs SPM's incremental build"
    echo ""
}

main "$@"
