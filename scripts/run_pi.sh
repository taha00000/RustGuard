#!/usr/bin/env bash
# Run the full RustGuard measurement set on a Raspberry Pi (or any aarch64
# Linux box) and leave CSVs ready to copy back to the analysis machine.
#
#   scripts/run_pi.sh [traces] [cpu]
#
# Defaults to 100k traces per primitive pinned to CPU 3. Application cores are
# noisy, so they need far more traces than the 3000 the microcontrollers use —
# the whole point of measuring here is to find out how much that noise costs in
# sensitivity.
set -euo pipefail

TRACES="${1:-100000}"
CPU="${2:-3}"
OUT="${OUT:-$HOME/rustguard-results}"
mkdir -p "$OUT"

cd "$(dirname "$0")/../pi-runner"

echo "=== environment ==="
MODEL="$(tr -d '\0' < /proc/device-tree/model 2>/dev/null || echo unknown)"
echo "model    : $MODEL"
echo "kernel   : $(uname -srm)"
echo "governor : $(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor 2>/dev/null || echo unknown)"
echo "paranoid : $(cat /proc/sys/kernel/perf_event_paranoid 2>/dev/null || echo unknown)"
# Crypto extensions change which code path the AES crates take, so record it.
if grep -qm1 aes /proc/cpuinfo; then
  echo "crypto   : ARMv8 crypto extensions PRESENT (aes in /proc/cpuinfo)"
else
  echo "crypto   : no ARMv8 crypto extensions (software AES path)"
fi

echo
echo "For the sharpest measurements, run these once (both are optional):"
echo "  sudo sysctl kernel.perf_event_paranoid=1    # enables true PMU cycle counts"
echo "  echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor"
echo

if ! command -v cargo >/dev/null; then
  echo "cargo not found. Install Rust:  curl https://sh.rustup.rs -sSf | sh" >&2
  exit 1
fi

echo "=== building ==="
cargo build --release --features leaky

TAG="$(echo "$MODEL" | tr '[:upper:] ' '[:lower:]-' | tr -cd 'a-z0-9-' | cut -c1-20)"
[ -z "$TAG" ] && TAG="pi"

for exp in verify keyed; do
  echo
  echo "=== $exp experiment ($TRACES traces/primitive) ==="
  ./target/release/pi-runner \
    --experiment "$exp" \
    --n "$TRACES" \
    --cpu "$CPU" \
    --board "$TAG" \
    --out "$OUT/${TAG}_${exp}.csv"
done

echo
echo "Done. Copy these back to the analysis machine:"
ls -la "$OUT"/*.csv
echo
echo "Then, on that machine:"
echo "  python capture/import_pi.py <file>.csv --opt O3 --outdir results/timing"
echo "  python analysis/matrix.py && python analysis/resolution.py"
