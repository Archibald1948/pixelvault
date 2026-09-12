#!/usr/bin/env bash
# crates/wasm 을 wasm-pack 으로 빌드해서 /pkg 에 npm 패키지 형태로 출력한다.
#
#   ./scripts/build-wasm.sh          # release 빌드
#   ./scripts/build-wasm.sh --dev    # 디버그 빌드 (빠르지만 크고 느림)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROFILE="--release"
if [[ "${1:-}" == "--dev" ]]; then
  PROFILE="--dev"
fi

# ── libwebp(C) 를 wasm 오브젝트로 묶으려면 llvm-ar 가 필요하다 ──────────────
# macOS 기본 ar 는 wasm 오브젝트의 심볼 인덱스를 만들지 못해서 링크 시 libwebp 함수를 못 찾는다.
# rustup 의 llvm-tools 컴포넌트에 들어 있는 llvm-ar 를 쓴다.
HOST="$(rustc -vV | sed -n 's/^host: //p')"
LLVM_AR="$(rustc --print sysroot)/lib/rustlib/$HOST/bin/llvm-ar"
if [[ ! -x "$LLVM_AR" ]]; then
  echo "❌ llvm-ar 를 찾을 수 없습니다: $LLVM_AR" >&2
  echo "   다음 명령으로 설치하세요:  rustup component add llvm-tools" >&2
  exit 1
fi
export AR_wasm32_unknown_unknown="$LLVM_AR"

# --target web : 번들러 없이도 쓸 수 있는 ES 모듈 + 명시적 init() 방식.
#                Next.js(webpack/Turbopack) 설정 차이를 피하는 가장 무난한 선택.
wasm-pack build "$ROOT/crates/wasm" \
  --target web \
  --out-dir "$ROOT/pkg" \
  --out-name pixelvault \
  "$PROFILE"

WASM="$ROOT/pkg/pixelvault_bg.wasm"

# ── 링크 검증 ─────────────────────────────────────────────────────────────
# C 함수가 링크되지 않으면 링커는 에러 대신 "env" 모듈의 JS import 로 남겨 버린다.
# 그러면 빌드는 성공한 것처럼 보이다가 브라우저에서 instantiate 할 때 터진다. 여기서 미리 잡는다.
node -e '
  const m = new WebAssembly.Module(require("fs").readFileSync(process.argv[1]));
  const bad = WebAssembly.Module.imports(m).filter(i => i.module === "env").map(i => i.name);
  if (bad.length) {
    console.error("❌ 링크되지 않은 C 심볼이 있습니다:", bad.join(", "));
    console.error("   (llvm-ar 설정 또는 crates/wasm-libc 에 구현이 빠졌는지 확인)");
    process.exit(1);
  }
' "$WASM"
RAW=$(wc -c < "$WASM" | tr -d ' ')
GZ=$(gzip -9 -c "$WASM" | wc -c | tr -d ' ')
echo
echo "✅ pkg/ 생성 완료"
printf "   pixelvault_bg.wasm: %'d bytes (gzip -9: %'d bytes)\n" "$RAW" "$GZ"

# ── 크기 예산 (Definition of Done: gzip 기준 500KB 이하) ──────────────────────
BUDGET="${WASM_GZIP_BUDGET:-512000}"
if [[ "$PROFILE" == "--release" && "$GZ" -gt "$BUDGET" ]]; then
  echo "❌ wasm 크기 예산 초과: gzip $GZ bytes > $BUDGET bytes" >&2
  echo "   twiggy top -n 30 pkg/pixelvault_bg.wasm 로 원인을 찾아보세요." >&2
  exit 1
fi
