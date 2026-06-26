#!/usr/bin/env bash
# Pull the C/asm baselines as submodules. Run once after cloning.
set -euo pipefail
cd "$(dirname "$0")"
git submodule add -f https://github.com/ascon/ascon-c.git vendor/ascon-c || true
git submodule add -f https://github.com/mupq/pqm4.git vendor/pqm4 || true
git submodule update --init --recursive
echo "Baselines fetched. See README.md for build steps."
