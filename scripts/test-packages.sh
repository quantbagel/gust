#!/bin/bash
# Test Gust against real Swift packages

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
GUST="$PROJECT_DIR/target/release/gust"
TEST_DIR="/tmp/gust-test-packages"

# Build release binary
echo "Building Gust..."
cd "$PROJECT_DIR"
cargo build --release 2>/dev/null

# Package list
PACKAGES=(
    "swift-argument-parser|https://github.com/apple/swift-argument-parser"
    "swift-log|https://github.com/apple/swift-log"
    "swift-collections|https://github.com/apple/swift-collections"
    "swift-algorithms|https://github.com/apple/swift-algorithms"
    "swift-numerics|https://github.com/apple/swift-numerics"
    "swift-system|https://github.com/apple/swift-system"
    "swift-atomics|https://github.com/apple/swift-atomics"
    "swift-crypto|https://github.com/apple/swift-crypto"
    "swift-nio|https://github.com/apple/swift-nio"
    "swift-protobuf|https://github.com/apple/swift-protobuf"
    "Alamofire|https://github.com/Alamofire/Alamofire"
    "Kingfisher|https://github.com/onevcat/Kingfisher"
    "SnapKit|https://github.com/SnapKit/SnapKit"
    "RxSwift|https://github.com/ReactiveX/RxSwift"
    "SwiftyJSON|https://github.com/SwiftyJSON/SwiftyJSON"
    "Quick|https://github.com/Quick/Quick"
    "Nimble|https://github.com/Quick/Nimble"
    "PromiseKit|https://github.com/mxcl/PromiseKit"
    "swift-syntax|https://github.com/apple/swift-syntax"
    "swift-markdown|https://github.com/apple/swift-markdown"
    "swift-async-algorithms|https://github.com/apple/swift-async-algorithms"
    "swift-http-types|https://github.com/apple/swift-http-types"
    "swift-metrics|https://github.com/apple/swift-metrics"
    "vapor|https://github.com/vapor/vapor"
    "fluent|https://github.com/vapor/fluent"
    "async-http-client|https://github.com/swift-server/async-http-client"
    "Moya|https://github.com/Moya/Moya"
    "swift-format|https://github.com/apple/swift-format"
    "grpc-swift|https://github.com/grpc/grpc-swift"
    "swift-openapi-generator|https://github.com/apple/swift-openapi-generator"
)

mkdir -p "$TEST_DIR"

echo ""
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║                Gust Integration Test Suite                   ║"
echo "╠══════════════════════════════════════════════════════════════╣"
echo "║ Testing ${#PACKAGES[@]} packages                                          ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""

PASSED=0
FAILED=0
FAILURES=()

for entry in "${PACKAGES[@]}"; do
    IFS='|' read -r name repo <<< "$entry"
    pkg_dir="$TEST_DIR/$name"

    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "Testing: $name"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    # Clone if needed
    if [ ! -d "$pkg_dir" ]; then
        echo "  → Cloning..."
        if git clone --depth 1 "$repo" "$pkg_dir" 2>/dev/null; then
            echo "  ✓ Cloned"
        else
            echo "  ✗ Clone failed"
            FAILED=$((FAILED + 1))
            FAILURES+=("$name: clone failed")
            continue
        fi
    else
        echo "  ✓ Using cached"
    fi

    # Test SwiftPM
    echo "  → Testing SwiftPM..."
    spm_start=$(python3 -c 'import time; print(int(time.time() * 1000))')
    if swift package dump-package --package-path "$pkg_dir" > /tmp/spm-output.json 2>/dev/null; then
        spm_end=$(python3 -c 'import time; print(int(time.time() * 1000))')
        spm_time=$((spm_end - spm_start))
        spm_deps=$(cat /tmp/spm-output.json | python3 -c "import sys,json; d=json.load(sys.stdin); print(len(d.get('dependencies',[])))" 2>/dev/null || echo "?")
        spm_targets=$(cat /tmp/spm-output.json | python3 -c "import sys,json; d=json.load(sys.stdin); print(len(d.get('targets',[])))" 2>/dev/null || echo "?")
        echo "  ✓ SwiftPM: ${spm_time}ms (deps: $spm_deps, targets: $spm_targets)"
        SPM_OK=1
    else
        echo "  ✗ SwiftPM parse failed"
        SPM_OK=0
    fi

    # Test Gust (via swift package dump-package since we use that internally)
    echo "  → Testing Gust manifest parsing..."
    gust_start=$(python3 -c 'import time; print(int(time.time() * 1000))')

    # Check if Package.swift exists
    if [ -f "$pkg_dir/Package.swift" ]; then
        # Try to migrate to see if parsing works
        cd "$pkg_dir"
        if $GUST migrate 2>/dev/null; then
            gust_end=$(python3 -c 'import time; print(int(time.time() * 1000))')
            gust_time=$((gust_end - gust_start))
            echo "  ✓ Gust: ${gust_time}ms (migrated to Gust.toml)"
            GUST_OK=1
            # Clean up
            rm -f "$pkg_dir/Gust.toml"
        else
            echo "  ✗ Gust parse failed"
            GUST_OK=0
        fi
        cd "$PROJECT_DIR"
    else
        echo "  ✗ No Package.swift found"
        GUST_OK=0
    fi

    # Summary for this package
    if [ "$SPM_OK" = "1" ] && [ "$GUST_OK" = "1" ]; then
        echo "  ✓ PASSED"
        PASSED=$((PASSED + 1))
    else
        echo "  ✗ FAILED"
        FAILED=$((FAILED + 1))
        if [ "$SPM_OK" = "0" ]; then
            FAILURES+=("$name: SwiftPM failed")
        fi
        if [ "$GUST_OK" = "0" ]; then
            FAILURES+=("$name: Gust failed")
        fi
    fi
    echo ""
done

echo ""
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║                         RESULTS                              ║"
echo "╠══════════════════════════════════════════════════════════════╣"
printf "║ Passed: %3d / %d                                            ║\n" "$PASSED" "${#PACKAGES[@]}"
printf "║ Failed: %3d                                                  ║\n" "$FAILED"

if [ ${#FAILURES[@]} -gt 0 ]; then
    echo "╠══════════════════════════════════════════════════════════════╣"
    echo "║ Failures:                                                    ║"
    for failure in "${FAILURES[@]}"; do
        printf "║   %-57s ║\n" "$failure"
    done
fi

echo "╚══════════════════════════════════════════════════════════════╝"

exit $FAILED
