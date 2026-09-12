# 00. 개발 환경 셋업 & 자주 쓰는 명령

## 한 번만 설치하면 되는 것

```bash
# 1) Rust 툴체인 (이미 있으면 생략)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 2) WebAssembly 컴파일 타깃
rustup target add wasm32-unknown-unknown

# 3) llvm-ar — libwebp(C)를 wasm 으로 묶을 때 필요 (자세한 이유: libwebp-on-wasm.md)
rustup component add llvm-tools

# 4) wasm-pack — Rust → wasm → npm 패키지까지 한 번에 만들어 주는 도구
cargo install wasm-pack      # 또는 https://rustwasm.github.io/wasm-pack/installer/

# 5) (선택) twiggy — wasm 파일에서 뭐가 용량을 먹는지 보여주는 도구
cargo install twiggy
```

Node.js 20+ 와 npm 이 필요합니다.

## 자주 쓰는 명령

| 하고 싶은 것 | 명령 |
|---|---|
| Rust 로직 테스트 (네이티브, 가장 빠름) | `cargo test` |
| 린트 (경고도 에러 취급) | `cargo clippy --workspace --all-targets -- -D warnings` |
| 코드 포맷 | `cargo fmt` |
| wasm 빌드 → `pkg/` | `./scripts/build-wasm.sh` |
| wasm 을 실제 wasm 런타임(Node)에서 테스트 | `./scripts/test-wasm.sh` |
| 데모 사이트 개발 서버 | `cd www && npm run dev` |
| 데모 사이트 프로덕션 빌드 | `cd www && npm run build && npm start` |
| 테스트 픽스처 재생성 | `cargo run -p pixelvault-core --example gen_fixtures` |
| 벤치마크용 큰 이미지 생성 | `cargo run --release -p pixelvault-core --example gen_bench_image` |
| Node 에서 wasm 처리 속도 측정 | `node bench/node-bench.mjs` |

> `www` 는 `pkg/` 를 `file:../pkg` 로 참조합니다. **Rust 코드를 바꿨으면 `./scripts/build-wasm.sh` 를 다시 돌려야** 웹에 반영됩니다.

## 폴더 구조

```
pixelvault/
├── Cargo.toml              # Cargo 워크스페이스 (크레이트 여러 개를 한 번에 관리)
├── .cargo/config.toml      # wasm32 빌드 플래그(SIMD), C 헤더 경로
├── crates/
│   ├── core/               # ★ 모든 이미지 로직. 순수 Rust, wasm 의존성 0. cargo test 로 검증
│   │   ├── src/            #   decode.rs / resize.rs / encode.rs / pipeline.rs / error.rs
│   │   ├── examples/       #   픽스처·벤치 이미지 생성기
│   │   └── tests/fixtures/ #   테스트용 샘플 이미지
│   ├── wasm/               # wasm-bindgen 바인딩. 타입 변환만, 로직 없음
│   └── wasm-libc/          # wasm32 에서 libwebp(C)가 쓸 malloc/free 등 최소 libc
├── pkg/                    # wasm-pack 출력 (gitignore)
├── www/                    # Next.js 데모
├── bench/                  # 벤치마크 스크립트
├── scripts/                # 빌드/테스트 스크립트
└── docs/                   # 지금 보고 있는 문서
```

### Cargo 용어 미니 사전

- **크레이트(crate)**: Rust 의 컴파일 단위 = npm 패키지 하나라고 생각하면 됨.
- **워크스페이스(workspace)**: 여러 크레이트를 한 레포에서 같이 관리. `target/` 폴더와 `Cargo.lock` 을 공유한다. (npm workspaces 와 비슷)
- **`Cargo.toml`**: `package.json` 에 해당. `[dependencies]` = dependencies, `[dev-dependencies]` = devDependencies.
- **feature**: 크레이트의 선택 기능 스위치. `default-features = false` 로 기본 기능을 끄고 필요한 것만 켤 수 있다 → wasm 크기 절약의 핵심.
- **`crate-type = ["cdylib"]`**: "다른 언어에서 부를 수 있는 라이브러리"로 빌드하라는 뜻. wasm 을 만들 때 필요.
