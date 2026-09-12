#!/usr/bin/env bash
# crates/wasm 의 wasm-bindgen-test 를 Node 에서 실행한다.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
HOST="$(rustc -vV | sed -n 's/^host: //p')"
export AR_wasm32_unknown_unknown="$(rustc --print sysroot)/lib/rustlib/$HOST/bin/llvm-ar"
wasm-pack test --node "$ROOT/crates/wasm" "$@"
