#!/usr/bin/env bash
# scripts/push_to_github.sh
# ─────────────────────────────────────────────────────────────────────────────
# Sets up the repository and pushes to GitHub in one command.
#
# Usage:
#   1. Create a NEW EMPTY repository on GitHub named "rustguard"
#      (do NOT initialise with README — the repo must be empty)
#   2. Run: bash scripts/push_to_github.sh your_github_username
#
# Example:
#   bash scripts/push_to_github.sh ta08451
# ─────────────────────────────────────────────────────────────────────────────

set -euo pipefail

GITHUB_USER="${1:-}"
REPO_NAME="rustguard"

if [ -z "$GITHUB_USER" ]; then
    echo "Usage: bash scripts/push_to_github.sh <your_github_username>"
    exit 1
fi

REMOTE_URL="https://github.com/${GITHUB_USER}/${REPO_NAME}.git"

echo "================================================"
echo " RustGuard → GitHub Push"
echo " Target: $REMOTE_URL"
echo "================================================"

# Work from repo root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

# Configure git if not already done
if ! git config user.email &>/dev/null; then
    echo "Enter your git email: "
    read GIT_EMAIL
    git config user.email "$GIT_EMAIL"
fi
if ! git config user.name &>/dev/null; then
    echo "Enter your name: "
    read GIT_NAME
    git config user.name "$GIT_NAME"
fi

# Initialise git repo if needed
if [ ! -d ".git" ]; then
    echo "Initialising git repository..."
    git init -b main
fi

# Stage everything
echo "Staging all files..."
git add .

# Commit
COMMIT_MSG="Initial release: RustGuard ASCON-128 IoT authentication library

- rustguard-core: no_std ASCON-128 AEAD + ASCON-HASH (24/24 tests pass)
- rustguard-pap:  IoT Packet Authentication Protocol with replay protection
- All 7 publication figures generated from real x86-64 benchmarks (N=10,000)
- IEEE conference paper (PDF + LaTeX source) included in paper/
- Raw benchmark data in results/raw/benchmark_x86_64.txt
- CI: GitHub Actions (test, clippy, fmt, cross-compile, size report)

Companion to: 'RustGuard: Design and Software Verification of a Memory-Safe,
no_std ASCON-128 Authenticated Encryption Library for IoT Packet Security'
Taha Hunaid Ali, Farhan Khan — Habib University, Karachi, Pakistan"

git commit -m "$COMMIT_MSG" || echo "(nothing new to commit)"

# Set remote
if git remote get-url origin &>/dev/null; then
    git remote set-url origin "$REMOTE_URL"
else
    git remote add origin "$REMOTE_URL"
fi

# Push
echo ""
echo "Pushing to $REMOTE_URL ..."
echo "(You may be prompted for your GitHub username and a Personal Access Token)"
echo "Create a token at: https://github.com/settings/tokens/new"
echo "Required scopes: repo (full)"
echo ""
git push -u origin main

echo ""
echo "================================================"
echo " Done! Repository live at:"
echo " https://github.com/${GITHUB_USER}/${REPO_NAME}"
echo "================================================"
echo ""
echo "Recommended next steps:"
echo "  1. Add repository description on GitHub:"
echo "     'Memory-safe no_std ASCON-128 AEAD for IoT packet authentication'"
echo "  2. Add topics: ascon, cryptography, iot, rust, no-std, embedded, lightweight-crypto"
echo "  3. Enable GitHub Pages on paper/ to serve the PDF"
