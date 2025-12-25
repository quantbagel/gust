#!/bin/bash
# Quick test on 5 packages

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
SWIFTY="$PROJECT_DIR/target/release/swifty"
TEST_DIR="/tmp/swifty-quick-test"

rm -rf "$TEST_DIR"
mkdir -p "$TEST_DIR"

PACKAGES=(
    "swift-argument-parser|https://github.com/apple/swift-argument-parser"
    "swift-log|https://github.com/apple/swift-log"
    "swift-collections|https://github.com/apple/swift-collections"
    "Alamofire|https://github.com/Alamofire/Alamofire"
    "SwiftyJSON|https://github.com/SwiftyJSON/SwiftyJSON"
)

echo "Quick test: ${#PACKAGES[@]} packages"
echo ""

PASSED=0
FAILED=0

for entry in "${PACKAGES[@]}"; do
    IFS='|' read -r name repo <<< "$entry"
    pkg_dir="$TEST_DIR/$name"

    echo "[$name]"

    # Clone
    if ! git clone --depth 1 -q "$repo" "$pkg_dir" 2>/dev/null; then
        echo "  ✗ Clone failed"
        FAILED=$((FAILED + 1))
        continue
    fi

    # Test SwiftPM parsing
    if swift package dump-package --package-path "$pkg_dir" > /tmp/spm.json 2>/dev/null; then
        deps=$(python3 -c "import json; print(len(json.load(open('/tmp/spm.json')).get('dependencies',[])))" 2>/dev/null)
        targets=$(python3 -c "import json; print(len(json.load(open('/tmp/spm.json')).get('targets',[])))" 2>/dev/null)
        echo "  SwiftPM: deps=$deps targets=$targets"
    else
        echo "  ✗ SwiftPM failed"
        FAILED=$((FAILED + 1))
        continue
    fi

    # Test Swifty migrate (tests parsing)
    cd "$pkg_dir"
    if $SWIFTY migrate 2>/dev/null; then
        echo "  Swifty: ✓ Parsed and migrated"
        cat Swifty.toml | head -10
        rm Swifty.toml
        PASSED=$((PASSED + 1))
    else
        echo "  ✗ Swifty failed"
        FAILED=$((FAILED + 1))
    fi
    cd "$PROJECT_DIR"
    echo ""
done

echo "Results: $PASSED passed, $FAILED failed"
