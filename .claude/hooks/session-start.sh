#!/bin/bash
# SessionStart hook: install the Kani model checker so the formal-verification
# proof crates (finance/escrow/kani-proofs, finance/token-swap/kani-proofs, ...)
# can be run with `cargo kani` in Claude Code on the web.
#
# Synchronous + idempotent. The Kani toolchain (verifier + CBMC) is large, so
# the first remote session pays the install cost; the container state is cached
# afterwards, making subsequent sessions fast.
set -euo pipefail

# Only needed in the remote (web) environment; local machines manage their own
# toolchains.
if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ]; then
  exit 0
fi

# Idempotent: nothing to do if Kani is already installed (e.g. cached container).
if command -v cargo-kani >/dev/null 2>&1 && cargo kani --version >/dev/null 2>&1; then
  echo "Kani already installed: $(cargo kani --version)"
  exit 0
fi

echo "Installing kani-verifier..."
cargo install --locked kani-verifier

echo "Running kani setup (downloads the CBMC toolchain)..."
kani setup

echo "Kani ready: $(cargo kani --version)"
