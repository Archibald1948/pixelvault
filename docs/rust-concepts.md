# Rust 개념 정리 — 이 프로젝트에 실제로 나온 것만

> TS/JS 개발자 관점에서, 이 레포의 실제 코드를 예로 들어 설명한다. 각 항목의 "어디서" 를 열어서 같이 보면 좋다.
>
> 이 문서는 **요약 사전**이다. 각 주제를 자세히 다룬 문서는 [`rust/`](./rust/README.md) 에 있다
> (문법 기초 · 소유권 · 트레잇 · 에러 · 컬렉션 · Cargo · unsafe/FFI · wasm-bindgen · 테스트 · **코드 전체 해설** · 성능).

## 1. 소유권 (Ownership) — 가장 중요한 하나

**규칙**: 모든 값에는 주인(변수)이 하나 있다. 주인이 스코프를 벗어나면 값은 **자동으로 해제**된다(GC 없음).
값을 다른 변수나 함수에 넘기면 주인이 **바뀐다(move)**. 이전 변수는 더 이상 쓸 수 없다.

```rust
let image = resize(decoded.image, w, h)?;  // decoded.image 의 소유권이 resize 함수로 이동
// decoded.image.width();  ← 컴파일 에러: value used after move
```

- 어디서: `crates/core/src/pipeline.rs`, `resize.rs`
- JS 와 비교: JS 는 참조가 하나라도 남아 있으면 GC 가 못 치운다. Rust 는 **컴파일러가 "여기서부터 아무도 안 씀"을 증명**하므로 즉시 해제된다. 12MP 원본(36MB)이 리사이즈 직후 풀리는 이유.
- 비슷한 개념이 JS 에도 있다: Worker 로 `ArrayBuffer` 를 **transfer** 하면 원래 쪽 버퍼가 비워진다(`docs/m3-worker.md`).

## 2. 빌림 (Borrowing): `&T` 와 `&mut T`

소유권을 넘기지 않고 **잠깐 빌려주기**.

| 표기 | 의미 | 동시에 |
|---|---|---|
| `&T` | 읽기 전용 빌림 | 여러 개 가능 |
| `&mut T` | 쓰기 가능한 빌림 | **딱 하나만**, 그동안 `&T` 도 불가 |

```rust
pub fn encode(image: &DynamicImage, ...) -> Result<Vec<u8>>   // 읽기만 → &
pub fn reset_orientation(raw_exif: &mut [u8])                  // 제자리에서 2바이트 수정 → &mut
```

- 어디서: `encode.rs`, `exif.rs`
- "읽는 사람 여럿 또는 쓰는 사람 하나" 규칙 덕분에 데이터 레이스가 **컴파일 단계에서** 막힌다.

## 3. `&[u8]` vs `Vec<u8>` — 슬라이스와 벡터

| | `Vec<u8>` | `&[u8]` |
|---|---|---|
| 정체 | 힙 버퍼를 **소유** (포인터 + 길이 + 용량) | 어딘가의 바이트를 **가리키는 뷰** (포인터 + 길이) |
| 만들 때 | 할당 | 복사 없음 |
| JS 로 치면 | 새 `Uint8Array` | `uint8array.subarray()` |
| 쓰는 곳 | 함수가 새로 만들어 돌려줄 때 | 함수가 읽기만 할 때 |

```rust
let (header, body) = rest.split_at(8);   // 슬라이스를 두 조각으로. 복사 없음
let payload = &body[..size];             // 범위로 잘라 보기. 복사 없음
```

- 어디서: `decode.rs`(입력), `webp_meta.rs`(청크 파싱)
- `&Vec<u8>` 은 자동으로 `&[u8]` 로 바뀐다(deref coercion). 그래서 함수 인자는 보통 `&[u8]` 로 받는 게 더 유연하다.

## 4. 라이프타임 `'a`

"이 참조는 **최소한 무엇만큼** 살아 있어야 한다"를 컴파일러에게 알려주는 표시.

```rust
struct Chunk<'a> {
    fourcc: [u8; 4],
    payload: &'a [u8],   // Chunk 는 원본 버퍼('a)보다 오래 살 수 없다
}
fn parse_chunks(webp: &[u8]) -> Result<Vec<Chunk<'_>>>  // '_ = "입력과 같은 수명" (생략 표기)
```

- 어디서: `webp_meta.rs`, `encode.rs` 의 `Metadata<'a>`
- C 의 dangling pointer(해제된 메모리를 가리키는 포인터) 버그를 **컴파일 에러**로 만든다.
- 대부분은 컴파일러가 추론해서 생략할 수 있다. 구조체가 참조를 품을 때만 직접 쓴다.

## 5. `Option<T>` 와 `Result<T, E>`

null 과 예외가 없는 대신 **타입으로** 표현한다.

```rust
Option<u32>                          // Some(1920) 또는 None  ≈ TS 의 number | undefined
Result<ProcessOutput, PixelVaultError> // Ok(값) 또는 Err(에러) ≈ throw 가능성을 타입에 적어 둔 것
```

자주 쓰는 도구 (`pipeline.rs`, `exif.rs`):

| 코드 | 의미 |
|---|---|
| `x?` | Ok/Some 이면 꺼내고, Err/None 이면 **즉시 return** |
| `opt.map(f)` | Some 이면 f 적용 |
| `opt.unwrap_or_default()` | None 이면 기본값 |
| `opt.as_deref()` | `Option<Vec<u8>>` → `Option<&[u8]>` (안을 빌려 보기) |
| `bool.then(\|\| x)` | true 면 Some(x) |
| `opt.transpose()` | `Option<Result<T>>` ↔ `Result<Option<T>>` |
| `opt.filter(\|x\| ...)` | 조건 안 맞으면 None |
| `let Some(x) = opt else { return ... };` | 꺼내거나, 안 되면 탈출 (let-else) |

`unwrap()` 은 "None/Err 면 panic" — 테스트 코드에서만 쓰고, 라이브러리 코드에서는 `?` 나 명시적 처리를 쓴다.

## 6. 에러: `thiserror` 와 `#[from]`

```rust
#[derive(Debug, thiserror::Error)]
pub enum PixelVaultError {
    #[error("image codec error: {0}")]
    Codec(#[from] image::ImageError),   // image::ImageError → ? 가 자동으로 Codec(...) 으로 감쌈
    #[error("image too large: {width}x{height} ...")]
    TooLarge { width: u32, height: u32, needed_bytes: u64, limit_bytes: u64 },
    ...
}
```

- 어디서: `error.rs`
- WASM 경계: `Result<T, JsError>` 로 반환하면 `?` 가 `PixelVaultError → JsError` 로 바꿔 주고, JS 에서는 `Error` 로 throw 된다 (`crates/wasm/src/lib.rs`).

## 7. `enum` 과 `match`

Rust 의 enum 은 값을 품을 수 있는 **태그드 유니언**이다 (TS 의 discriminated union 과 같다).

```rust
match image {
    DynamicImage::ImageRgb8(buf)  => webp::Encoder::from_rgb(buf.as_raw(), w, h),
    DynamicImage::ImageRgba8(buf) => webp::Encoder::from_rgba(buf.as_raw(), w, h),
    _ => unreachable!(),
}
```

- `match` 는 **모든 경우를 처리했는지 컴파일러가 검사**한다. `OutputFormat` 에 `Avif` 를 추가하면, 처리 안 한 `match` 가 전부 컴파일 에러로 드러난다.
- `f @ (A | B)` — 패턴에 맞은 값을 `f` 라는 이름으로 잡기 (`decode.rs`).

## 8. 트레잇 (Trait) — 인터페이스

```rust
impl FromStr for OutputFormat { ... }        // → "webp".parse::<OutputFormat>() 가능
impl Default for ProcessOptions { ... }      // → ProcessOptions { quality: 90, ..Default::default() }
impl fmt::Display for OutputFormat { ... }   // → format!("{fmt}") 가능

fn attach_metadata<E: ImageEncoder>(encoder: &mut E, ...)  // ImageEncoder 를 구현한 아무 타입이나
```

- 어디서: `encode.rs`, `pipeline.rs`
- `#[derive(Debug, Clone, Copy, PartialEq)]` 는 흔한 트레잇 구현을 자동 생성하는 매크로.

## 9. 제네릭과 단형화 (Monomorphization)

```rust
fn resize_typed<P: PixelTrait, Px: Pixel<Subpixel = u8>>(src: &ImageBuffer<Px, Vec<u8>>, ...) { ... }

resize_typed::<U8x3, _>(...)   // 컴파일러가 U8x3 전용 사본을 만든다
resize_typed::<U8x4, _>(...)   // U8x4 전용 사본
```

- TS 제네릭은 타입 검사 후 사라지지만, Rust 제네릭은 **쓰인 타입마다 코드가 따로 생성**된다. 그래서 빠르다(가상 호출 없음).
- 반대로, 쓰이지 않은 타입의 코드는 아예 안 생긴다 → 이걸 이용해 wasm 을 **1.74 MB → 0.98 MB** 로 줄였다 (`docs/m1-core-pipeline.md`).

## 10. `Cow` — 필요할 때만 복사

```rust
fn as_rgb8_or_rgba8(image: &DynamicImage) -> Cow<'_, DynamicImage> {
    match image {
        ImageRgb8(_) | ImageRgba8(_) => Cow::Borrowed(image),   // 대부분: 빌려서 그대로
        other => Cow::Owned(convert(other.clone())),             // 가끔: 새로 만들어 소유
    }
}
```

- 어디서: `encode.rs`. "보통은 빌려 쓰고, 드물게만 새로 만든다"를 타입으로 표현.

## 11. 정수 오버플로와 타입 변환

```rust
let needed = u64::from(width).saturating_mul(u64::from(height)).saturating_mul(4);
```

- 디버그 빌드에서 오버플로는 panic, **릴리스 빌드에서는 조용히 wrap-around** 된다. 크기 검사처럼 보안에 중요한 계산은 `checked_*`(None 반환) / `saturating_*`(최댓값 고정)을 쓴다.
- `as` 는 무조건 변환(잘림 가능), `u64::from` / `try_from` 은 안전한 변환. (`decode.rs`)
- JS→Rust 경계에서도 같은 문제: wasm-bindgen 은 JS number 를 u8 로 **잘라서** 넘긴다(300 → 44). 그래서 TS 쪽에서 범위 검증 (`packages/pixelvault/src/core.ts`).

## 12. `unsafe` 와 FFI

```rust
#[unsafe(no_mangle)]
pub unsafe extern "C" fn malloc(size: usize) -> *mut c_void { ... }
```

- `extern "C"`: C 호출 규약. `no_mangle`: 이름을 `malloc` 그대로 내보냄.
- `*mut c_void`: raw 포인터. 빌림 검사를 받지 않는다. 역참조는 `unsafe { }` 안에서만.
- `unsafe` 는 "컴파일러가 검사 못 하는 약속을 **내가** 지킨다"는 서명. 그래서 `# Safety` 문서로 그 약속을 적는다.
- 어디서: `crates/wasm-libc/src/lib.rs` — 프로젝트의 `unsafe` 는 전부 이 크레이트에 격리되어 있다.

## 13. 조건부 컴파일 `#[cfg(...)]`

```rust
#![cfg(target_arch = "wasm32")]           // 이 크레이트 전체를 wasm 에서만
#[cfg(target_arch = "wasm32")]
extern crate pixelvault_wasm_libc as _;   // wasm 일 때만 링크
#[cfg(test)] mod tests { ... }            // cargo test 일 때만
```

Cargo.toml 에서도: `[target.'cfg(target_arch = "wasm32")'.dependencies]`.

## 14. 모듈과 경로

```rust
pub mod decode;              // src/decode.rs 를 모듈로
use crate::error::Result;    // crate:: = 이 크레이트의 루트
use super::*;                // super:: = 부모 모듈 (테스트 모듈에서 자주)
::blurhash::encode(...)      // 앞의 :: = 외부 크레이트 (같은 이름의 모듈과 구분)
```

## 15. 테스트

```rust
#[cfg(test)]
mod tests {
    use super::*;
    const JPEG: &[u8] = include_bytes!("../tests/fixtures/landscape.jpg"); // 컴파일 시점에 파일을 바이트로 박아 넣음

    #[test]
    fn decodes_jpeg_as_rgb8() {
        let d = decode(JPEG).unwrap();
        assert_eq!(d.image.color(), ColorType::Rgb8);
    }
}
```

- 테스트는 같은 파일 안에 둔다 (비공개 함수도 테스트 가능).
- `assert!(matches!(err, PixelVaultError::TooLarge { width: 640, .. }))` — enum 모양으로 검사.
- 이 프로젝트에서 테스트가 잡아낸 것: `u64` 오버플로(M1), BlurHash 에 대한 잘못된 가정(M2), JPEG 디코더의 관대함(M1).

## 이 레포 안에서 더 읽을 것

| 주제 | 문서 |
|---|---|
| 문법 기초부터 차근차근 | [rust/01-basics.md](./rust/01-basics.md) |
| 소유권·빌림·라이프타임 심화 | [rust/02-ownership.md](./rust/02-ownership.md) |
| 트레잇·제네릭·단형화 | [rust/03-traits-generics.md](./rust/03-traits-generics.md) |
| 에러 처리 전략 | [rust/04-error-handling.md](./rust/04-error-handling.md) |
| 컬렉션·이터레이터·클로저 | [rust/05-collections-iterators.md](./rust/05-collections-iterators.md) |
| 모듈·Cargo·feature·프로필 | [rust/06-modules-cargo.md](./rust/06-modules-cargo.md) |
| unsafe 와 C 연동 | [rust/07-unsafe-ffi.md](./rust/07-unsafe-ffi.md) |
| wasm-bindgen 내부 동작 | [rust/08-wasm-bindgen.md](./rust/08-wasm-bindgen.md) |
| 테스트와 도구 | [rust/09-testing-tooling.md](./rust/09-testing-tooling.md) |
| **코드 전체 해설 (2,150줄)** | [rust/10-code-walkthrough.md](./rust/10-code-walkthrough.md) |
| 성능과 메모리 측정 | [rust/11-performance-memory.md](./rust/11-performance-memory.md) |

## 더 공부하려면

- [The Rust Programming Language (한국어)](https://doc.rust-kr.org/) — 4장(소유권), 6장(enum), 9장(에러), 10장(제네릭·트레잇·라이프타임)
- [Rust and WebAssembly](https://rustwasm.github.io/docs/book/)
- [wasm-bindgen Guide](https://rustwasm.github.io/docs/wasm-bindgen/)
