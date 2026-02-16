#!/usr/bin/env bash
# ──────────────────────────────────────────────────────────────────────────────
# hexz — Common helpers for tools/scripts/ and tools/bench/
# ──────────────────────────────────────────────────────────────────────────────
# Sourcing: From tools/scripts/ use:  source "$SCRIPT_DIR/lib/common.sh"
#           From tools/bench/ use:    source "$SCRIPT_DIR/../scripts/lib/common.sh"
# ──────────────────────────────────────────────────────────────────────────────

# Project root (three levels up from tools/scripts/lib/)
get_project_root() {
    local common_dir
    common_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    echo "$(cd "${common_dir}/../../.." && pwd)"
}

# Logging
info()  { printf '\033[36m[info]\033[0m %s\n' "$*"; }
ok()    { printf '\033[32m[ok]\033[0m %s\n' "$*"; }
warn()  { printf '\033[33m[warn]\033[0m %s\n' "$*"; }
fail()  { printf '\033[31m[fail]\033[0m %s\n' "$*" >&2; exit 1; }

# Require commands to be present
check_cmd() {
    local cmd
    for cmd in "$@"; do
        if ! command -v "$cmd" &>/dev/null; then
            fail "Required command not found: $cmd"
        fi
    done
}

# Build hexz release binary if missing
ensure_build() {
    local bin="${1:?usage: ensure_build BIN}"
    local root
    root="$(get_project_root)"
    if [[ ! -x "$bin" ]]; then
        info "Building hexz (release)..."
        (cd "$root" && cargo build --release --workspace) || fail "Build failed"
    fi
}
