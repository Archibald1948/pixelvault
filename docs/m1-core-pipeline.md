# M1 — 코어 파이프라인: decode → resize → encode

> 목표: JPEG/PNG/WebP 를 받아서 줄이고 WebP/JPEG/PNG 로 다시 굽는다.
> 모든 로직은 `crates/core` 에 두고 **네이티브 `cargo test`** 로 검증한다.

## 파일별 역할

| 파일 | 입력 → 출력 | 핵심 |
|---|---|---|
| `error.rs` | — | `thiserror` 로 에러 enum 하나. 모든 단계가 `Result<T, PixelVaultError>` 반환 |
| `decode.rs` | `&[u8]` → `DynamicImage` | 매직 바이트로 포맷 판별, **디코드 전** 메모리 상한 검사, RGB8/RGBA8 로 정규화 |
| `resize.rs` | `DynamicImage` → `DynamicImage` | 비율 유지 + 확대 금지 크기 계산, Lanczos3 + SIMD |
| `encode.rs` | `&DynamicImage` → `Vec<u8>` | WebP(libwebp), JPEG(알파→흰 배경 합성), PNG(무손실) |
| `pipeline.rs` | `&[u8]` + 옵션 → 결과 | 위 셋을 조립하는 오케스트레이터 |

## 크레이트 선택 이유

| 목적 | 크레이트 | 왜 |
|---|---|---|
| 디코드/인코드 | `image` (jpeg, png, webp feature 만) | Rust 이미지 생태계의 표준. **`default-features = false`** 로 GIF/TIFF/AVIF 등을 빼서 wasm 크기 절약 |
| 리사이즈 | `fast_image_resize` | `image::imageops::resize` 보다 수 배 빠름 (아래 설명) |
| JPEG 인코딩 | `jpeg-encoder` (M4 에서 교체) | `image` 내장 인코더는 4:2:2 + 고정 허프만 테이블이라 같은 품질에서 ~30% 큼. 4:2:0 + 최적화 허프만 지원 → [M4 문서](./m4-benchmark.md) |
| WebP 인코딩 | `webp` (libwebp C 라이브러리 바인딩) | `image` 의 WebP 인코더는 **무손실만** 지원 → 사진이 오히려 커진다. 손실 압축 + quality 조절은 libwebp 가 사실상 유일한 선택. 크롬의 `canvas.toBlob('image/webp')` 도 내부적으로 libwebp 를 쓴다 |
| 에러 | `thiserror` | `Display`/`Error` 구현을 derive 로 자동 생성 |

> libwebp 는 C 코드라서 wasm 으로 빌드하는 게 이 프로젝트에서 가장 까다로운 부분이었다. → [libwebp-on-wasm.md](./libwebp-on-wasm.md)

## 학습 포인트 1: `&[u8]` vs `Vec<u8>` — 제로카피는 어디서 일어나나

```rust
pub fn process(input: &[u8], options: &ProcessOptions) -> Result<ProcessOutput>
//                    ^^^^^ 빌린 슬라이스: "어딘가에 있는 바이트들을 읽기만 할게"
pub fn encode(image: &DynamicImage, ...) -> Result<Vec<u8>>
//                                                 ^^^^^^^ 소유한 버퍼: "새로 만든 바이트, 이제 네 거야"
```

- **`&[u8]`** = (포인터, 길이) 두 값뿐인 **뷰**. 만들 때 복사가 없다. JS 의 `Uint8Array.subarray()` 와 비슷.
- **`Vec<u8>`** = 힙에 할당된 버퍼를 **소유**. 스코프를 벗어나면 자동 해제된다(GC 없이).
- 규칙: **읽기만 하면 `&[u8]`, 새로 만들어서 넘겨줘야 하면 `Vec<u8>`**.

이 파이프라인에서 복사가 실제로 일어나는 곳:

```
JS Uint8Array ──(① 복사: JS 힙 → wasm 메모리)──▶ &[u8] input
   input ──(복사 없음: Cursor 로 감싸서 읽기만)──▶ 디코더
   디코더 ──(새 할당: 픽셀 버퍼)──▶ DynamicImage
   DynamicImage ──(RGB8 이면 복사 없음: 소유권만 이동)──▶ 정규화
   ──(새 할당: 작은 픽셀 버퍼, 원본은 여기서 해제)──▶ resize 결과
   ──(빌려서 읽기만)──▶ 인코더 ──(새 할당)──▶ Vec<u8>
Vec<u8> ──(② 복사: wasm 메모리 → JS 힙)──▶ JS Uint8Array
```

JS↔WASM 경계를 넘을 때 ①② 두 번만 복사된다. 그래서 API 를 "큰 파일은 한 번 넘기고, 결과도 한 번만 받는" 형태로 설계했다.
(여러 번 왔다갔다 하는 API — 예: `decode()` 따로, `resize()` 따로 JS 에서 부르기 — 는 매번 수십 MB 픽셀을 복사하게 된다)

## 학습 포인트 2: `DynamicImage` 의 소유권이 파이프라인을 타고 흐르는 모습

`pipeline.rs`:

```rust
let decoded = decode(input)?;                                         // decoded 가 픽셀을 소유
let image = resize(decoded.image, options.max_width, options.max_height)?; // 소유권이 resize 로 "이동(move)"
// 여기서 decoded.image 를 쓰면 컴파일 에러! (use of moved value)
let bytes = encode(&image, options.format, options.quality)?;         // & = 빌려주기만
Ok(ProcessOutput { width: image.width(), ..., bytes })                // 빌려줬으니 image 는 아직 내 것
```

`resize` 는 `DynamicImage` 를 **값으로** 받는다:

```rust
pub fn resize(image: DynamicImage, ...) -> Result<DynamicImage> {
    if 크기가_이미_작으면 { return Ok(image); }   // 받은 걸 그대로 돌려줌 → 복사 0
    ...새 버퍼에 리사이즈...
    Ok(dst)                                      // 함수 끝: 원본 image 가 drop(해제)됨
}
```

- 12MP 사진의 원본 픽셀은 36 MB. 이걸 **리사이즈 직후 바로 해제**할 수 있는 건, 호출자가 원본을 더 이상 쓸 수 없다는 걸 컴파일러가 보장하기 때문이다.
- JS 였다면 `decoded` 변수가 살아 있는 한 GC 가 원본을 못 치운다.
- `resize_is_noop_when_already_small` 테스트는 **버퍼 주소가 같은지** 비교해서 "복사 없이 소유권만 돌아왔다"를 검증한다.

`encode` 는 `&DynamicImage`(빌림)를 받는다. 인코딩은 읽기만 하면 되고, 호출자는 인코딩 후에도 `image.width()` 를 써야 하니까.

## 학습 포인트 3: `Result` + `?` 로 에러 전파

```rust
pub fn process(input: &[u8], options: &ProcessOptions) -> Result<ProcessOutput> {
    validate(options)?;          // Err 면 즉시 return Err(...)
    let decoded = decode(input)?;
    ...
}
```

- Rust 에는 예외(throw)가 없다. 실패할 수 있는 함수는 반환 타입이 `Result<성공값, 에러>` 다.
- `?` 는 "Ok 면 안의 값을 꺼내고, Err 면 **그대로 호출자에게 return**" 하는 축약.
- `#[from]` 을 붙인 에러 variant 는 `?` 가 자동 변환해 준다:

```rust
#[derive(Debug, thiserror::Error)]
pub enum PixelVaultError {
    #[error("image codec error: {0}")]
    Codec(#[from] image::ImageError),   // image::ImageError 가 나오면 ? 가 Codec(...) 으로 감싸 줌
    ...
}
```

WASM 경계에서는 `crates/wasm` 이 반환 타입을 `Result<_, JsError>` 로 선언한다.
`JsError` 는 `std::error::Error` 를 구현한 모든 타입에서 `From` 변환을 제공하므로, 여기서도 `?` 한 글자로 끝난다:

```rust
let out = pixelvault_core::process(input, &options)?;  // PixelVaultError → JsError 자동 변환
```

JS 에서는 그냥 `try { processImageRaw(...) } catch (e) { e.message }` 로 받는다.

## 학습 포인트 4: 왜 `fast_image_resize` 가 `image::imageops::resize` 보다 빠른가

1. **SIMD**: `.cargo/config.toml` 의 `-C target-feature=+simd128` 로 WebAssembly SIMD 를 켰다. 128비트 레지스터 하나에 픽셀 채널 여러 개를 넣어 **한 명령어로 동시에** 곱하고 더한다.
2. **분리 가능한(separable) 컨볼루션**: 2차원 필터(Lanczos3 커널, 가로 6×세로 6 탭)를 그대로 적용하면 픽셀당 36번 곱셈이 필요하다. 가로 방향 1차원 → 세로 방향 1차원 두 패스로 나누면 6+6 = 12번.
3. **정수 고정소수점 연산 + 계수 미리 계산**: 필터 가중치를 한 번만 계산해 두고, `f32` 대신 정수로 곱해 SIMD 폭을 더 넓게 쓴다.
4. **알파 premultiply 를 SIMD 로**: 투명 경계가 검게 번지지 않게 하는 처리까지 벡터화되어 있다.

`image::imageops::resize` 는 픽셀마다 `f32` 로 커널을 계산하는 범용 구현이라 이런 최적화가 없다.

## 학습 포인트 5: 제네릭과 wasm 크기 — 1.74 MB → 0.98 MB

처음엔 `fast_image_resize` 의 `image` feature 로 `DynamicImage` 를 그대로 넘겼다. 그랬더니 wasm 이 1.74 MB 였다.
`twiggy top` 으로 보니 `.rodata`(정적 데이터)가 645 KB 로 37% — 그중 **512 KB 가 16비트 알파 역수 테이블**(`RECIP_ALPHA16`)이었다.

원인: `DynamicImage` 는 런타임에 픽셀 타입이 결정되는 enum 이라, 라이브러리가 **모든 픽셀 타입(8/16비트, float, 1~4채널)의 코드를 다 준비**해야 한다. 우리는 RGB8/RGBA8 만 쓰는데도.

해결: 픽셀 타입을 **제네릭 타입 파라미터**로 고정했다.

```rust
match image {
    DynamicImage::ImageRgb8(src)  => resize_typed::<U8x3, _>(&src, w, h),
    DynamicImage::ImageRgba8(src) => resize_typed::<U8x4, _>(&src, w, h),
    ...
}
```

Rust 제네릭은 **단형화(monomorphization)** 된다: 컴파일러가 실제로 쓰인 타입(`U8x3`, `U8x4`)별로 함수 사본을 만들고, 안 쓰인 타입의 코드는 아예 생성하지 않는다.
결과: **1.74 MB → 0.98 MB (gzip 378 KB)**. 한 줄 설계 차이가 바이너리 크기를 절반으로 만들었다.

> 교훈: wasm 크기가 이상하면 추측하지 말고 `twiggy top <파일.wasm>` 부터.

## 학습 포인트 6: 메모리 상한 — 디코드 "전에" 검사

wasm32 는 포인터가 32비트라 **주소 공간이 4 GiB** 가 한계다. 50MP 사진을 RGBA 로 풀면 200 MB. 그 상태로 리사이즈 버퍼, 인코딩 버퍼가 동시에 올라간다.

```rust
let decoder = reader.into_decoder()?;       // 헤더만 읽음 (가로·세로 알 수 있음)
let (width, height) = decoder.dimensions();
check_decoded_size(width, height, limit)?;  // 가로 × 세로 × 4 > 1 GiB 면 여기서 거절
let image = DynamicImage::from_decoder(decoder)?;  // 이제야 픽셀을 푼다
```

- 픽셀을 푼 **다음에** 검사하면 이미 늦다(메모리 폭발로 탭이 죽음).
- `u32 × u32` 는 넘치기 쉬워서 `u64` 로 넓혀서 곱한다. 그런데 테스트를 짜 보니 `u32::MAX × u32::MAX × 4` 는 **`u64` 도 넘쳤다**. 릴리스 빌드에서 정수 오버플로는 panic 없이 조용히 wrap-around 되므로 검사가 뚫릴 수 있다 → `saturating_mul`(넘치면 최댓값에 고정)로 수정. **테스트가 실제 버그를 잡은 사례.**

## 인코딩 세부 결정

- **JPEG + 알파**: JPEG 에는 알파 채널이 없다. 알파를 그냥 버리면 투명 픽셀(보통 RGB=0,0,0)이 **검은색**으로 나온다. 흰 배경에 합성: `out = c·a + 255·(1−a)`.
- **RGBA 리사이즈는 premultiplied alpha**: 투명 픽셀의 RGB 값(대개 검정)이 섞여 들어가 경계가 어두워지는 문제를 막는다. `resize_premultiplies_alpha` 테스트로 검증.
- **WebP 최대 16383px**: libwebp 제한. 넘으면 C 쪽 에러 대신 명확한 `InvalidOption` 을 먼저 돌려준다.
- **PNG 는 quality 무시**: 무손실 포맷이라 품질 개념이 없다.

## 테스트

`cargo test -p pixelvault-core` — 27개. 픽스처는 `crates/core/examples/gen_fixtures.rs` 가 생성한다(재현 가능하게).

| 픽스처 | 용도 |
|---|---|
| `landscape.jpg` 640×480 | 기본 JPEG. 상하좌우 비대칭 (M2 회전 테스트에도 사용) |
| `transparent.png` 256×256 RGBA | 알파 처리 |
| `gray16.png` 64×64 16비트 흑백 | RGB8 정규화 |
| `photo.webp` 320×240 | WebP 입력 |

wasm 런타임 검증: `./scripts/test-wasm.sh` — `wasm-bindgen-test` 로 Node 에서 3개. 특히 알파 WebP 인코딩은 libwebp 의 무손실 경로를 타면서 `qsort`/`calloc` 같은 **직접 구현한 libc 함수**를 실제로 호출하므로, 셈이 제대로 동작하는지 확인하는 역할.

## 측정: wasm 크기 vs 속도 (opt-level)

12MP JPEG(6.7 MB) → 1920px, Node 22 (Apple Silicon), 3회 중앙값:

| 설정 | wasm | gzip | → WebP | → JPEG | → PNG |
|---|---:|---:|---:|---:|---:|
| **opt-level=3, LTO** ✅ | 985 KB | **378 KB** | **605 ms** | **282 ms** | **583 ms** |
| opt-level="s", LTO | 759 KB | 310 KB | 782 ms | 377 ms | 806 ms |
| opt-level="z", LTO | 742 KB | 310 KB | 958 ms | 541 ms | 989 ms |
| opt-level=3, SIMD 끔 | 926 KB | 361 KB | 638 ms | 322 ms | 653 ms |

> M4 에서 JPEG 인코더를 `jpeg-encoder` 로 바꾼 뒤 → JPEG **252 ms / 183 KB** (253 KB 에서 감소), wasm gzip 411 KB.

- 스펙은 `opt-level="z"` 를 제안했지만, **3 으로도 gzip 500 KB 예산 안에 들어오고** "z" 보다 1.6배 빠르다 → 3 채택.
- "z"/"s" 는 인라이닝·루프 펼치기를 포기해서 크기를 줄인다. 이미지 처리처럼 **뜨거운 루프가 대부분인 코드**에서는 손해가 크다.
- 교훈: 크기 최적화 옵션은 "무조건 z"가 아니라 **예산 안에서 가장 빠른 것**을 측정으로 고른다.

## 다음 단계(M2)에서 바뀌는 것

- EXIF Orientation 을 픽셀에 반영 (아이폰 세로 사진이 눕는 문제)
- EXIF 제거 옵션 (`stripExif`), BlurHash 생성 옵션
