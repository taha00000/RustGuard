#!/usr/bin/env bash
# scripts/run_benchmarks.sh
# Run the RustGuard benchmark suite and save raw output.
# Usage: bash scripts/run_benchmarks.sh [output_file]

set -euo pipefail

OUTFILE="${1:-results/raw/benchmark_$(date +%Y%m%d_%H%M%S).txt}"
BENCH_DIR="$(dirname "$0")/../benchmark"
WORKSPACE="$(dirname "$0")/.."

echo "=========================================="
echo " RustGuard Benchmark Suite"
echo " $(date)"
echo " Rust: $(rustc --version)"
echo " Host: $(uname -srm)"
echo "=========================================="

# Create a temporary benchmark binary if it doesn't exist as a workspace member
if [ ! -d "$BENCH_DIR" ]; then
    echo "Creating benchmark binary..."
    mkdir -p "$BENCH_DIR/src"
    cat > "$BENCH_DIR/Cargo.toml" << 'TOML'
[package]
name = "benchmark"
version = "0.1.0"
edition = "2021"

[dependencies]
rustguard-core = { path = "../rustguard-core" }
rustguard-pap  = { path = "../rustguard-pap" }

[profile.release]
opt-level     = 3
lto           = true
codegen-units = 1
TOML

    cp "$(dirname "$0")/benchmark_main.rs" "$BENCH_DIR/src/main.rs" 2>/dev/null || {
        echo "ERROR: benchmark_main.rs not found in scripts/. See README for setup."
        exit 1
    }
fi

echo "Building benchmark binary (release, LTO)..."
cd "$WORKSPACE"
cargo build --release -p benchmark 2>&1

echo ""
echo "Running benchmarks (N=10,000 per configuration)..."
echo ""

mkdir -p "$(dirname "$OUTFILE")"
./target/release/benchmark | tee "$OUTFILE"

echo ""
echo "Raw results saved to: $OUTFILE"
echo ""
echo "To regenerate figures, run:"
echo "  python3 scripts/generate_figures.py"
