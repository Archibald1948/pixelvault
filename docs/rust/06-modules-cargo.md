# 06. 모듈 · 크레이트 · Cargo

"파일을 어떻게 나누고, 의존성을 어떻게 관리하고, 빌드를 어떻게 조절하는가".

## 1. 크레이트와 워크스페이스

- **크레이트(crate)** = 컴파일 단위 ≈ npm 패키지 하나. 라이브러리(`src/lib.rs`) 또는 실행 파일(`src/main.rs`).
- **워크스페이스(workspace)** = 크레이트 여러 개를 한 레포에서 관리. `target/` 과 `Cargo.lock` 을 공유한다.

```toml
# Cargo.toml (레포 루트)
[workspace]
resolver = "3"
members = ["crates/core", "crates/wasm", "crates/wasm-libc"]

[workspace.package]        # 멤버들이 공유할 메타데이터
version = "0.1.0"
edition = "2024"
license = "MIT"
```

```toml
# crates/core/Cargo.toml
[package]
name = "pixelvault-core"
version.workspace = true    # 워크스페이스 값을 상속
edition.workspace = true
publish = false             # crates.io 에 실수로 올라가지 않게
```

**에디션(edition)** 은 언어의 "판"이다(2015/2018/2021/2024). 같은 컴파일러로 서로 다른 에디션 크레이트를 섞어 쓸 수 있다.
2024 에디션에서 바뀐 것 중 이 레포에 보이는 것: `#[unsafe(no_mangle)]`, `unsafe fn` 안에서도 `unsafe {}` 블록을 요구.

## 2. 모듈: 파일이 곧 모듈

```rust
// crates/core/src/lib.rs
pub mod blurhash;    // → src/blurhash.rs
pub mod decode;      // → src/decode.rs
pub mod encode;
pub mod error;
pub mod exif;
pub mod pipeline;
pub mod resize;
pub mod webp_meta;

pub use encode::OutputFormat;                     // 재수출: 사용자가 짧게 쓸 수 있게
pub use pipeline::{ProcessOptions, ProcessOutput, process};
```

`mod x;` 를 적어야 그 파일이 **컴파일에 포함된다**. (TS 처럼 import 만으로 자동으로 딸려 오지 않는다)

### 경로 규칙

```rust
use crate::error::Result;        // crate:: = 내 크레이트 루트
use super::*;                    // super:: = 부모 모듈 (테스트 모듈에서 흔함)
use self::inner::X;              // self:: = 현재 모듈
use image::DynamicImage;         // 외부 크레이트는 이름부터
::blurhash::encode(...)          // 앞의 :: = "외부 크레이트임을 명시" (동명의 모듈과 구분)
```

마지막 형태는 `crates/core/src/blurhash.rs` 에서 실제로 필요했다. 모듈 이름(`blurhash`)과 크레이트 이름(`blurhash`)이 같아서다.

### 가시성

| 표기 | 범위 |
|---|---|
| (없음) | 이 모듈과 자식 모듈 |
| `pub` | 어디서나 |
| `pub(crate)` | 이 크레이트 안에서만 |
| `pub(super)` | 부모 모듈까지 |

기본이 비공개라, 내부 헬퍼(`encode_webp`, `flatten_on_white`, `parse_chunks`)는 그냥 `fn` 으로 두면 외부 API 가 되지 않는다.
테스트는 같은 파일 안의 `mod tests` 에 있으므로 **비공개 함수도 그대로 테스트할 수 있다.**

## 3. 의존성과 feature

```toml
[dependencies]
image = { version = "0.25", default-features = false, features = ["jpeg", "png", "webp"] }
fast_image_resize = "6.1"
webp = { version = "0.3", default-features = false }
kamadak-exif = "0.6"       # 크레이트 이름과 코드에서 쓰는 이름(exif)이 다를 수 있다
blurhash = { version = "0.2", default-features = false }
jpeg-encoder = "0.7"
thiserror = "2.0"
```

- **feature** = 크레이트의 선택 기능 스위치. `default-features = false` 로 기본값을 끄고 필요한 것만 켠다.
  `image` 의 기본값은 GIF/TIFF/AVIF/ICO... 전부인데, 우리는 3개만 필요하다 → **wasm 크기가 크게 줄어든다.**
- 반대로 feature 하나가 거대한 코드를 끌고 오기도 한다: `fast_image_resize` 의 `image` feature 를 켰더니
  wasm 이 750 KB 늘었다(→ 03 문서의 단형화 이야기).
- 버전 `"0.25"` 는 `^0.25` 를 뜻한다(0.x 에서는 0.25.x 안에서만 올라간다). 실제로 쓰인 버전은 `Cargo.lock` 에 고정된다.

### 타깃별 의존성

```toml
# wasm32 에서만 필요한 의존성
[target.'cfg(target_arch = "wasm32")'.dependencies]
pixelvault-wasm-libc = { path = "../wasm-libc" }
```

```toml
[dev-dependencies]          # 테스트/예제에서만 쓰는 의존성 (배포물에 안 들어감)
wasm-bindgen-test = "0.3"
```

## 4. 조건부 컴파일 `cfg`

```rust
#![cfg(target_arch = "wasm32")]        // 파일(크레이트) 전체를 wasm 에서만 컴파일
#[cfg(target_arch = "wasm32")]
extern crate pixelvault_wasm_libc as _;  // wasm 일 때만 링크에 포함 (as _ = 이름은 안 쓰고 링크만)
#[cfg(test)]
mod tests { ... }                       // cargo test 일 때만
```

`cfg` 조건: `target_arch`, `target_os`, `feature = "x"`, `test`, `debug_assertions` 등.
조합은 `#[cfg(all(a, b))]`, `#[cfg(any(a, b))]`, `#[cfg(not(a))]`.

## 5. 빌드 프로필

```toml
[profile.release]
opt-level = 3        # 0~3, "s"(크기), "z"(더 작게)
lto = true           # 링크 타임 최적화: 크레이트 경계를 넘어 인라이닝 + 죽은 코드 제거
codegen-units = 1    # 병렬 컴파일 포기 → 최적화 기회 최대
```

- `cargo build` = dev 프로필(최적화 없음, 디버그 정보, 오버플로 검사 켜짐)
- `cargo build --release` = release 프로필
- 이미지 처리는 release 와 dev 의 속도 차이가 10배 이상 난다. 벤치마크는 항상 release 로.
- 각 옵션의 실측 효과는 `docs/m1-core-pipeline.md` 표 참고.

## 6. `.cargo/config.toml` — 빌드 환경 설정

```toml
[target.wasm32-unknown-unknown]
rustflags = ["-C", "target-feature=+simd128"]

[env]
C_INCLUDE_PATH = { value = "crates/wasm-libc/include", relative = true }
```

- `rustflags`: 특정 타깃에만 컴파일러 플래그를 준다.
- `[env]`: 빌드 중 환경변수를 설정한다. `relative = true` 면 이 설정 파일 기준 경로로 바뀐다.
  (여기서는 C 컴파일러가 우리 libc 셈 헤더를 찾도록 쓰였다 → 07 문서)

## 7. 빌드 스크립트(`build.rs`)와 `-sys` 크레이트

이 레포가 직접 쓰지는 않지만, 의존성 중 `libwebp-sys` 가 쓴다. `build.rs` 는 컴파일 전에 실행되는 Rust 프로그램으로,
C 소스를 컴파일하거나 코드를 생성한다. `cc` 크레이트가 여기서 `clang` 을 부르고, 그때 우리가 `.cargo/config.toml` 로
설정한 `C_INCLUDE_PATH` 와 `AR_wasm32_unknown_unknown` 이 쓰인다.

관례: `xxx-sys` 크레이트는 C 라이브러리의 **날것 바인딩**, `xxx` 크레이트는 그 위의 **안전한 래퍼**다.
(`libwebp-sys` → `webp`)

## 8. 예제(examples)와 테스트 폴더

```
crates/core/
├── src/            # 라이브러리 코드 (+ #[cfg(test)] 단위 테스트)
├── examples/       # cargo run --example gen_fixtures
└── tests/          # 통합 테스트 (여기선 픽스처 파일 보관용으로만 사용)
```

```bash
cargo run -p pixelvault-core --example gen_fixtures            # 픽스처 재생성
cargo run --release -p pixelvault-core --example gen_bench_image
```

`examples/` 는 배포물에 포함되지 않으면서 `cargo build --examples` 로 컴파일 검사를 받는다.
"문서화된 실행 가능한 스크립트"를 두기 좋은 자리다.

## 9. 자주 쓰는 Cargo 명령

```bash
cargo check                 # 타입 검사만 (제일 빠름)
cargo build [--release]
cargo test [-p 크레이트] [필터]
cargo clippy --workspace --all-targets -- -D warnings   # 린트, 경고를 에러로
cargo fmt [--check]         # 포매팅
cargo doc --open            # /// 주석으로 문서 생성
cargo tree                  # 의존성 트리
cargo add <크레이트>        # 의존성 추가 (Cargo.toml 자동 수정)
cargo clean -p <크레이트>   # 특정 크레이트만 다시 빌드하게
```

## 다음

→ [07. unsafe 와 FFI](./07-unsafe-ffi.md)
