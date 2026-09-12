# 10. 코드 전체 해설

`crates/` 의 Rust 코드 2,150줄을 순서대로 읽는다. 각 절은 **파일을 열어 놓고** 보는 걸 전제로 한다.
모르는 문법이 나오면 해당 장(01~09)으로 돌아가자.

읽는 순서는 데이터가 흐르는 순서다:

```
lib.rs (지도) → error.rs → decode.rs → resize.rs → exif.rs → encode.rs → webp_meta.rs
→ blurhash.rs → pipeline.rs (조립) → wasm/lib.rs (JS 경계) → wasm-libc (C 받침대) → examples
```

---

## 1. `crates/core/src/lib.rs` (40줄) — 크레이트의 지도

```rust
pub mod blurhash;  pub mod decode;  pub mod encode;  pub mod error;
pub mod exif;      pub mod pipeline; pub mod resize;  pub mod webp_meta;

pub use encode::OutputFormat;
pub use error::{PixelVaultError, Result};
pub use pipeline::{ProcessOptions, ProcessOutput, process};

#[cfg(target_arch = "wasm32")]
extern crate pixelvault_wasm_libc as _;
```

- `mod` 로 각 파일을 크레이트에 포함시키고, 자주 쓰는 타입은 `pub use` 로 **재수출**한다.
  사용자는 `pixelvault_core::OutputFormat` 처럼 짧게 쓸 수 있다(실제 위치는 `encode` 모듈).
- 마지막 줄이 중요하다: `pixelvault-wasm-libc` 는 우리 코드가 **부르는 함수가 하나도 없다**
  (libwebp 가 C 쪽에서 부른다). 그래서 명시적으로 `extern crate ... as _;` 를 적지 않으면
  컴파일러가 "안 쓰는 크레이트"로 보고 링크에서 빼 버린다 → `malloc` 을 못 찾아 wasm 이 깨진다.
  `as _` 는 "이름은 안 쓸 거고 링크만 해 달라"는 뜻.

---

## 2. `error.rs` (44줄) — 에러 하나로 통일

```rust
pub type Result<T> = std::result::Result<T, PixelVaultError>;

#[derive(Debug, thiserror::Error)]
pub enum PixelVaultError {
    #[error("unsupported input format (supported: JPEG, PNG, WebP)")]
    UnsupportedInput,
    #[error("image too large: {width}x{height} would need {needed_bytes} bytes decoded (limit {limit_bytes})")]
    TooLarge { width: u32, height: u32, needed_bytes: u64, limit_bytes: u64 },
    #[error("invalid option: {0}")]
    InvalidOption(String),
    #[error("image codec error: {0}")]
    Codec(#[from] image::ImageError),
    #[error("resize failed: {0}")]
    Resize(#[from] fast_image_resize::ResizeError),
    #[error("jpeg encode failed: {0}")]
    JpegEncode(#[from] jpeg_encoder::EncodingError),
    #[error("webp encode failed: {0}")]
    WebpEncode(String),
}
```

- `type Result<T>` 별칭 덕분에 모든 함수가 `-> Result<T>` 만 적으면 된다.
- `#[from]` 이 붙은 세 개는 `?` 가 **자동 변환**한다. `WebpEncode` 만 문자열인 이유:
  `webp` 크레이트의 에러 타입이 `std::error::Error` 를 구현하지 않아 `#[from]` 을 쓸 수 없다.
- 관련 장: [04. 에러 처리](./04-error-handling.md)

---

## 3. `decode.rs` (191줄) — 바이트 → 픽셀

### 핵심 함수

```rust
pub fn decode_with_limit(input: &[u8], max_decoded_bytes: u64) -> Result<Decoded> {
    let mut reader = ImageReader::new(Cursor::new(input)).with_guessed_format().map_err(ImageError::from)?;

    let format = match reader.format() {
        Some(f @ (ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::WebP)) => f,
        _ => return Err(PixelVaultError::UnsupportedInput),
    };

    let mut limits = Limits::default();
    limits.max_alloc = Some(max_decoded_bytes);
    reader.limits(limits);

    let mut decoder = reader.into_decoder()?;          // ← 여기까지 헤더만 읽었다
    let (width, height) = decoder.dimensions();
    check_decoded_size(width, height, max_decoded_bytes)?;

    let icc = decoder.icc_profile().ok().flatten();
    let exif = decoder.exif_metadata().ok().flatten();

    let image = DynamicImage::from_decoder(decoder)?;   // ← 이제야 픽셀을 푼다
    Ok(Decoded { image: to_8bit_rgb_or_rgba(image), format, icc, exif })
}
```

읽을 포인트:

1. **`Cursor::new(input)`** — `&[u8]` 에 "현재 읽는 위치"만 붙여서 `Read + Seek` 로 만든다. 복사 없음.
2. **매직 바이트로 포맷 판별** — 확장자를 믿지 않는다. `f @ (A | B | C)` 는 "이 중 하나면 그 값을 `f` 로".
3. **순서가 안전장치다** — 헤더에서 크기를 읽고 검사한 **뒤에** 픽셀을 푼다. 반대로 하면 메모리가 터진 뒤다.
4. **메타데이터를 먼저 꺼낸다** — `DynamicImage::from_decoder(decoder)` 가 `decoder` 의 **소유권을 가져가므로**,
   그 뒤에는 `decoder.icc_profile()` 을 부를 수 없다. 컴파일러가 순서를 강제한다.
5. **`.ok().flatten()`** — `Result<Option<T>>` 를 `Option<T>` 로. "메타데이터가 깨졌다고 변환 전체를 실패시키지 않는다"는 정책.

### 오버플로를 막는 검사

```rust
pub fn check_decoded_size(width: u32, height: u32, limit_bytes: u64) -> Result<()> {
    let needed_bytes = u64::from(width).saturating_mul(u64::from(height)).saturating_mul(4);
    if needed_bytes > limit_bytes { return Err(PixelVaultError::TooLarge { ... }); }
    Ok(())
}
```

`u32 × u32` 는 `u64` 로 넓혀도 4를 곱하면 넘칠 수 있다(`u32::MAX²×4`). `saturating_mul` 은 넘치면 최댓값에 고정한다.
**테스트가 잡아낸 실제 버그**다.

### 픽셀 형식 정규화

```rust
pub fn to_8bit_rgb_or_rgba(image: DynamicImage) -> DynamicImage {
    match image {
        DynamicImage::ImageRgb8(_) | DynamicImage::ImageRgba8(_) => image,   // 이동만, 복사 없음
        other if other.color().has_alpha() => DynamicImage::ImageRgba8(other.into_rgba8()),
        other => DynamicImage::ImageRgb8(other.into_rgb8()),
    }
}
```

- 값으로 받아 값으로 돌려준다 → 이미 원하는 형식이면 **아무 일도 안 일어난다**.
- `other if 조건` 은 **match 가드**. 패턴 + 추가 조건.
- 이후 모든 단계가 "RGB8 아니면 RGBA8"만 상정할 수 있게 되어, 코드와 wasm 크기가 함께 줄어든다.

---

## 4. `resize.rs` (182줄) — 픽셀 → 더 작은 픽셀

### 크기 계산 (순수 함수)

```rust
pub fn fit_within(width: u32, height: u32, max_width: Option<u32>, max_height: Option<u32>) -> (u32, u32) {
    let scale_w = max_width.map_or(1.0, |m| f64::from(m) / f64::from(width));
    let scale_h = max_height.map_or(1.0, |m| f64::from(m) / f64::from(height));
    let scale = scale_w.min(scale_h).min(1.0);      // 1.0 과의 min = 확대 금지
    if scale >= 1.0 { return (width, height); }
    (( f64::from(width) * scale).round().max(1.0) as u32, ...)
}
```

- `map_or(기본값, 함수)` = `map(f).unwrap_or(기본값)` 의 짧은 형태.
- 부동소수 계산 후 `as u32` 로 자르는데, `.round().max(1.0)` 으로 **0 이 되는 경우를 막는다**(테스트 있음).
- I/O 도 할당도 없는 순수 함수라 테스트가 쉽다 — 이런 계산은 항상 분리해 두는 게 좋다.

### 두 가지 리사이즈 API

```rust
pub fn resize(image: DynamicImage, ...) -> Result<DynamicImage>        // 소유권을 가져감
pub fn resized_copy(image: &DynamicImage, ...) -> Result<DynamicImage> // 빌리기만
```

시그니처가 곧 의도의 표현이다. `resize` 는 "원본은 이제 필요 없음"(파이프라인),
`resized_copy` 는 "원본도 계속 쓸 것"(BlurHash 용 32px 사본).

### 타입을 고정한 내부 함수

```rust
fn resize_typed<P, Px>(src: &ImageBuffer<Px, Vec<u8>>, width: u32, height: u32) -> Result<ImageBuffer<Px, Vec<u8>>>
where P: PixelTrait, Px: Pixel<Subpixel = u8>,
{
    let src_view = TypedImageRef::<P>::from_buffer(src.width(), src.height(), src.as_raw()).map_err(buffer_error)?;
    let mut dst = ImageBuffer::<Px, Vec<u8>>::new(width, height);
    let mut dst_view = TypedImage::<P>::from_buffer(width, height, &mut dst).map_err(buffer_error)?;

    let options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3));
    Resizer::new().resize_typed(&src_view, &mut dst_view, &options)?;
    Ok(dst)
}
```

- `TypedImageRef` 는 `&[u8]` 를 "P 타입 픽셀 배열"로 **다시 해석한 뷰**다(복사 없음).
- `dst` 를 먼저 만들고 `dst_view` 로 감싸서 결과를 직접 쓰게 한다 → 중간 버퍼가 없다.
- 이 함수 하나가 wasm 크기를 750 KB 줄였다. 이유는 [03. 단형화](./03-traits-generics.md#4-단형화monomorphization--제네릭이-코드-크기에-미치는-영향).

---

## 5. `exif.rs` (133줄) — 방향과 개인정보

```rust
pub struct ExifSummary { pub orientation: u8, pub has_gps: bool }

pub fn summarize(raw_exif: &[u8]) -> ExifSummary {
    let Ok(exif) = Reader::new().read_raw(raw_exif.to_vec()) else {
        return ExifSummary::default();          // 깨진 EXIF 는 "정보 없음"으로
    };
    let orientation = exif.get_field(Tag::Orientation, In::PRIMARY)
        .and_then(|field| field.value.get_uint(0))
        .and_then(|v| u8::try_from(v).ok())
        .filter(|v| (1..=8).contains(v))
        .unwrap_or(1);
    let has_gps = exif.fields().any(|f| f.tag.context() == Context::Gps);
    ExifSummary { orientation, has_gps }
}
```

- `let Ok(x) = ... else { ... }` — 실패하면 기본값으로 빠져나간다.
- `Option` 체인으로 "있고, 숫자로 변환되고, 1~8 범위인" 값만 통과시킨다. 하나라도 실패하면 `1`(그대로).
- `read_raw` 가 `Vec<u8>` 을 요구해서 여기서 한 번 복사한다(작은 데이터라 문제없음).

```rust
pub fn swaps_dimensions(orientation: u8) -> bool { matches!(orientation, 5..=8) }

pub fn apply_orientation(mut image: DynamicImage, orientation: u8) -> DynamicImage {
    if let Some(o) = Orientation::from_exif(orientation) { image.apply_orientation(o); }
    image
}

pub fn reset_orientation(raw_exif: &mut [u8]) {
    let _ = Orientation::remove_from_exif_chunk(raw_exif);
}
```

- `mut image: DynamicImage` — **인자에 붙은 `mut`** 는 "내가 받은 이 값을 내 안에서 바꾸겠다"는 뜻이다.
  호출자와는 무관하다(`&mut` 와 헷갈리지 말 것).
- `reset_orientation` 은 `&mut [u8]` 로 받아 **2바이트만 제자리에서** 고친다. EXIF 를 유지할 때 이중 회전을 막는 장치.
- `let _ = ...` 는 "반환값을 의도적으로 버린다"(`#[must_use]` 경고 억제).

---

## 6. `encode.rs` (344줄) — 픽셀 → 바이트

### 포맷 enum

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat { #[default] Webp, Jpeg, Png }
impl OutputFormat { pub fn as_str(self) -> &'static str; pub fn mime_type(self) -> &'static str; }
impl fmt::Display for OutputFormat { ... }
impl FromStr for OutputFormat { ... }
```

문자열은 **경계에서 한 번만** enum 으로 바꾸고, 내부는 전부 타입으로 다룬다.

### 진입점

```rust
pub fn encode(image: &DynamicImage, format: OutputFormat, quality: u8) -> Result<Vec<u8>> {
    encode_with_metadata(image, format, quality, Metadata::default())
}

pub fn encode_with_metadata(image: &DynamicImage, format: OutputFormat, quality: u8, meta: Metadata<'_>) -> Result<Vec<u8>> {
    if !(1..=100).contains(&quality) { return Err(...); }
    let image = as_rgb8_or_rgba8(image);          // Cow<DynamicImage>
    match format {
        OutputFormat::Webp => {
            let webp = encode_webp(&image, quality)?;
            crate::webp_meta::add_metadata(webp, image.width(), image.height(), meta.icc, meta.exif)
        }
        OutputFormat::Jpeg => encode_jpeg(&image, quality, meta),
        OutputFormat::Png => encode_png(&image, meta),
    }
}
```

- `Metadata<'a> { icc: Option<&'a [u8]>, exif: Option<&'a [u8]> }` — 참조만 담은 작은 `Copy` 구조체.
- `&image` 로 `Cow` 를 넘기면 자동 역참조(`Deref`)로 `&DynamicImage` 가 된다.

### WebP

```rust
let encoder = match image {
    DynamicImage::ImageRgba8(buf) => webp::Encoder::from_rgba(buf.as_raw(), w, h),
    DynamicImage::ImageRgb8(buf)  => webp::Encoder::from_rgb(buf.as_raw(), w, h),
    _ => unreachable!("as_rgb8_or_rgba8 guarantees RGB8/RGBA8"),
};
let memory = encoder.encode_simple(false, f32::from(quality))
    .map_err(|e| PixelVaultError::WebpEncode(format!("{e:?}")))?;
Ok(memory.to_vec())
```

- `buf.as_raw()` 는 내부 `Vec<u8>` 을 빌린다 → C 로 넘어가는 포인터도 복사 없이 만들어진다.
- `memory` 는 **C 가 malloc 한 버퍼를 감싼 타입**. `to_vec()` 으로 Rust 메모리에 복사하고,
  함수가 끝나면서 `Drop` 이 C 쪽 `WebPFree` 를 부른다 → 해제를 잊을 수 없다.
- `unreachable!` 은 "여기 오면 우리 코드 버그"라는 선언.

### JPEG

```rust
let flattened;
let rgb: &RgbImage = match image {
    DynamicImage::ImageRgb8(buf) => buf,
    DynamicImage::ImageRgba8(buf) => { flattened = flatten_on_white(buf); &flattened }
    _ => unreachable!(),
};
```

`flattened` 를 **미리 선언만 해 두고** match 안에서 대입하는 관용구다.
match 블록 안에서 만든 값은 블록이 끝나면 죽으므로, 바깥 스코프에 자리를 만들어 둬야 참조를 돌려줄 수 있다.
(TS 에서는 그냥 `let x; if (...) x = ...` 이지만, Rust 는 이렇게 해야 라이프타임이 맞는다)

```rust
let mut encoder = jpeg_encoder::Encoder::new(&mut out, quality);
encoder.set_optimized_huffman_tables(true);
if let Some(icc) = meta.icc { encoder.add_icc_profile(icc)?; }
if let Some(exif) = meta.exif { encoder.add_exif_metadata(exif)?; }
encoder.encode(rgb.as_raw(), w as u16, h as u16, jpeg_encoder::ColorType::Rgb)?;
```

- `w as u16` 은 위에서 `JPEG_MAX_DIMENSION`(65535) 검사를 했기 때문에 안전하다. **`as` 를 쓰기 전에 검사한다.**
- 4:2:0 서브샘플링 + 최적화 허프만 → 같은 품질에서 24% 작다([M4](../m4-benchmark.md)).

### 알파 합성

```rust
fn flatten_on_white(rgba: &RgbaImage) -> RgbImage {
    let mut out = RgbImage::new(rgba.width(), rgba.height());
    for (src, dst) in rgba.as_raw().chunks_exact(4).zip(out.chunks_exact_mut(3)) {
        let a = u16::from(src[3]);
        for c in 0..3 {
            dst[c] = ((u16::from(src[c]) * a + 255 * (255 - a) + 127) / 255) as u8;
        }
    }
    out
}
```

- `u8` 끼리 곱하면 넘치므로 `u16` 으로 올려서 계산한다. `+127` 은 반올림.
- `chunks_exact(4)`/`chunks_exact_mut(3)` + `zip` 으로 인덱스 계산 없이 픽셀 단위 순회.

---

## 7. `webp_meta.rs` (230줄) — 바이트 조립 연습

```rust
struct Chunk<'a> { fourcc: [u8; 4], payload: &'a [u8] }

pub fn add_metadata(webp: Vec<u8>, width: u32, height: u32, icc: Option<&[u8]>, exif: Option<&[u8]>) -> Result<Vec<u8>> {
    if icc.is_none() && exif.is_none() { return Ok(webp); }   // 할 일 없으면 그대로 (복사 0)
    let chunks = parse_chunks(&webp)?;
    ...
}
```

- **입력을 `Vec<u8>` 로 받는 이유**: 메타데이터가 없으면 그대로 돌려주기 위해서다. `&[u8]` 로 받았다면
  아무 일 안 할 때도 `to_vec()` 복사가 필요했을 것이다. *소유권을 받는 것이 최적화가 되는 사례.*
- `Chunk<'a>` 의 `payload` 는 입력 버퍼를 가리키는 슬라이스다. 라이프타임 덕분에 원본보다 오래 살 수 없다.

파싱 루프:

```rust
let mut rest = &webp[12..];
while !rest.is_empty() {
    if rest.len() < 8 { return Err(malformed("truncated chunk header")); }
    let (header, body) = rest.split_at(8);
    let fourcc: [u8; 4] = header[0..4].try_into().expect("slice of len 4");
    let size = u32::from_le_bytes(header[4..8].try_into().expect("slice of len 4")) as usize;
    let padded = size + (size & 1);                 // RIFF 청크는 짝수 길이로 정렬
    if body.len() < size { return Err(malformed("chunk larger than file")); }
    chunks.push(Chunk { fourcc, payload: &body[..size] });
    rest = &body[padded.min(body.len())..];
}
```

- **모든 자르기 전에 길이를 검사한다.** 안 하면 잘못된 파일에 panic 한다(wasm 에서는 탭이 죽는다).
- `try_into()` 로 `&[u8]` → `[u8; 4]` 변환. 길이가 다르면 실패하는 변환이라 `Result` 다.
- `from_le_bytes` = 리틀엔디언 바이트 → 정수. 파일 포맷 파싱의 기본기.
- `size & 1` 은 홀짝 검사(비트 연산).

쓰기:

```rust
out.extend_from_slice(b"RIFF");
out.extend_from_slice(&[0; 4]);       // 전체 크기는 나중에 채운다
out.extend_from_slice(b"WEBP");
...
let riff_size = u32::try_from(out.len() - 8).map_err(|_| too_big())?;
out[4..8].copy_from_slice(&riff_size.to_le_bytes());   // 되돌아가서 채우기
```

"자리를 비워 두고 나중에 채우기"는 바이너리 포맷을 쓸 때 흔한 패턴이다.

---

## 8. `blurhash.rs` (84줄) — 작지만 결정이 많은 파일

```rust
const SAMPLE_SIZE: u32 = 32;

pub fn encode(image: &DynamicImage) -> Result<String> {
    let small = resize::resized_copy(image, Some(SAMPLE_SIZE), Some(SAMPLE_SIZE))?;
    let rgba = small.to_rgba8();
    let (cx, cy) = components_for(rgba.width(), rgba.height());
    ::blurhash::encode(cx, cy, rgba.width(), rgba.height(), rgba.as_raw())
        .map_err(|e| PixelVaultError::InvalidOption(format!("blurhash: {e}")))
}
```

- `::blurhash` 의 맨 앞 `::` — 이 **모듈 이름도 `blurhash`** 라서, 외부 크레이트임을 명시해야 한다.
- 32px 로 줄여서 계산하는 이유는 비용(가로×세로×성분 수)에 비해 결과가 거의 같기 때문.
- `resized_copy`(빌림)를 쓰는 이유: 원본은 인코딩에 계속 필요하다.

---

## 9. `pipeline.rs` (369줄, 절반은 테스트) — 조립

```rust
pub fn process(input: &[u8], options: &ProcessOptions) -> Result<ProcessOutput> {
    validate(options)?;

    let Decoded { image, format: source_format, icc, exif } = decode(input)?;   // 구조 분해

    let summary = exif.as_deref().map(exif::summarize).unwrap_or_default();

    let (max_w, max_h) = if exif::swaps_dimensions(summary.orientation) {
        (options.max_height, options.max_width)      // 90도 회전이면 제한을 뒤바꾼다
    } else {
        (options.max_width, options.max_height)
    };
    let image = resize(image, max_w, max_h)?;                       // 먼저 줄이고
    let image = exif::apply_orientation(image, summary.orientation); // 작은 걸 돌린다

    let blurhash = options.blurhash.then(|| crate::blurhash::encode(&image)).transpose()?;

    let kept_exif = if options.strip_exif { None } else {
        exif.map(|mut raw| { exif::reset_orientation(&mut raw); raw })
    };
    let kept_exif = kept_exif.filter(|e| options.format != OutputFormat::Jpeg || e.len() <= JPEG_MAX_EXIF);

    let meta = Metadata { icc: icc.as_deref(), exif: kept_exif.as_deref() };
    let bytes = encode_with_metadata(&image, options.format, options.quality, meta)?;

    Ok(ProcessOutput {
        width: image.width(), height: image.height(),
        format: options.format, source_format,
        original_bytes: input.len(), output_bytes: bytes.len(),
        bytes,                                   // ← 마지막에 이동 (위의 bytes.len() 보다 뒤)
        blurhash,
        source_orientation: summary.orientation, had_gps: summary.has_gps,
        exif_kept: kept_exif.is_some(),
    })
}
```

읽을 포인트:

1. **구조 분해**로 `Decoded` 의 필드를 각각 독립적인 변수로 꺼낸다. `image` 만 이동시키고 `exif` 는 남겨 둘 수 있다.
2. **섀도잉**(`let image = ...` 두 번)으로 단계별 변환을 표현한다. 이전 단계의 값은 접근 불가 → 실수 방지.
3. **순서가 성능**: 리사이즈 → 회전. 36 MB 대신 8 MB 를 돌린다.
4. **`bytes` 를 구조체에 마지막에 넣는다**: `output_bytes: bytes.len()` 이 이동보다 먼저 와야 한다.
   순서를 바꾸면 "이동한 값을 사용" 에러가 난다(컴파일러가 잡아 준다).
5. `exif.map(|mut raw| { ...; raw })` — `Option` 안의 `Vec` 을 **꺼내서 수정하고 다시 넣는** 관용구.
   클로저 인자에 `mut` 를 붙이면 그 안에서 값을 바꿀 수 있다.

### 테스트가 검증하는 완료 기준

```rust
fn iphone_portrait_is_not_lying_down()   // 3개 포맷 × 크기 + 픽셀 색으로 방향 검증
fn gps_is_removed_by_default()           // 결과물을 kamadak-exif 로 다시 열어 EXIF 부재 확인
fn keeping_exif_resets_orientation_to_avoid_double_rotation()
fn icc_profile_is_preserved_in_every_format()
fn max_width_applies_to_displayed_orientation()
```

특히 첫 번째는 "해가 왼쪽 위, 땅이 아래"를 **실제 픽셀 색으로** 확인한다. 크기만 봐서는 잡을 수 없는 버그가 있기 때문.

---

## 10. `crates/wasm/src/lib.rs` (143줄) — JS 경계

```rust
#[wasm_bindgen]
pub struct RawProcessResult { bytes: Vec<u8>, width: u32, ..., blurhash: Option<String> }

#[wasm_bindgen]
impl RawProcessResult {
    #[wasm_bindgen(getter)] pub fn width(&self) -> u32 { self.width }
    #[wasm_bindgen(getter, js_name = mimeType)] pub fn mime_type(&self) -> String { ... }
    #[wasm_bindgen(js_name = takeBytes)] pub fn take_bytes(&mut self) -> Vec<u8> { std::mem::take(&mut self.bytes) }
}

#[wasm_bindgen(js_name = processImageRaw)]
pub fn process_image_raw(input: &[u8], max_width: Option<u32>, ..., blurhash: bool) -> Result<RawProcessResult, JsError> {
    let options = ProcessOptions { max_width, max_height, format: format.parse()?, quality, strip_exif, blurhash };
    let out = pixelvault_core::process(input, &options)?;
    Ok(RawProcessResult { bytes: out.bytes, ... })
}
```

- **로직이 한 줄도 없다.** 타입 변환과 위임만 한다. 이 규칙 덕분에 거의 모든 코드를 네이티브에서 테스트할 수 있다.
- `format.parse()?` 한 줄에 문자열 → enum 변환과 에러 전파가 모두 들어 있다.
- `Option<u32>` 는 JS 의 `undefined` 와 자동으로 대응된다.
- 자세한 동작은 [08. wasm-bindgen](./08-wasm-bindgen.md).

---

## 11. `crates/wasm-libc/` (194줄 + 헤더 6개) — C 를 위한 받침대

```rust
#![cfg(target_arch = "wasm32")]     // 이 크레이트 전체가 wasm 에서만 존재
```

- `include/*.h` — libwebp 가 필요로 하는 선언만 담은 최소 헤더. 비-wasm 에서는 `#include_next` 로 진짜 헤더에 위임.
- `src/lib.rs` — `malloc`/`calloc`/`realloc`/`free`/`qsort`/`bsearch`/`abort` 구현.
- 이 프로젝트의 **모든 `unsafe` 가 여기 모여 있다.** 자세한 해설은 [07. unsafe 와 FFI](./07-unsafe-ffi.md),
  배경 이야기는 [libwebp-on-wasm](../libwebp-on-wasm.md).

---

## 12. `examples/gen_fixtures.rs` (138줄) — 테스트 데이터도 코드다

```rust
fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    ...
    let upright = DynamicImage::ImageRgb8(landscape(300, 400));
    let stored = upright.rotate270().to_rgb8();     // 센서가 저장하는 방향으로 미리 돌려 둔다
    let exif = iphone_exif();                        // Orientation=6 + GPS
    let mut enc = JpegEncoder::new_with_quality(&mut jpeg, 92);
    enc.set_exif_metadata(exif).unwrap();
    enc.write_image(stored.as_raw(), stored.width(), stored.height(), ExtendedColorType::Rgb8).unwrap();
}
```

- `env!("CARGO_MANIFEST_DIR")` — **컴파일 시점**에 크레이트 경로를 문자열로 박아 넣는 매크로.
  실행 위치와 무관하게 파일을 찾을 수 있다.
- 아이폰 사진을 흉내 내려고 "보여야 할 모습"을 만든 뒤 반대로 돌려서 저장한다. 테스트가 검증할 정답이 명확해진다.
- `iphone_exif()` 는 `exif::experimental::Writer` 로 TIFF 구조를 직접 만든다. GPS 좌표는 유리수 3개(도/분/초).

`gen_bench_image.rs` 는 같은 아이디어로 "사진 같은" 큰 이미지를 만든다.
저주파(그라데이션) + 중간 주파수(무늬) + 고주파(해시 노이즈)를 섞는 이유는, **노이즈가 없으면 JPEG/WebP 가
비현실적으로 잘 압축되어 벤치마크가 무의미해지기** 때문이다.

---

## 13. 전체를 관통하는 설계 원칙 정리

| 원칙 | 코드에서의 모습 |
|---|---|
| 경계에서 타입을 좁힌다 | 문자열 → `OutputFormat`, 바이트 → `DynamicImage`(RGB8/RGBA8 두 가지로 정규화) |
| 실패 가능성을 타입에 적는다 | 모든 단계가 `Result<T>`, 없을 수 있는 값은 `Option<T>` |
| 소유권으로 의도를 표현한다 | `resize(image)` vs `resized_copy(&image)`, `encode(&image)` |
| 위험한 검사는 먼저 한다 | 디코드 전 크기 검사, 자르기 전 길이 검사, `as` 전 범위 검사 |
| unsafe 는 격리한다 | `crates/wasm-libc` 하나에 전부 |
| wasm 은 껍데기로 유지한다 | 로직 0줄 → 네이티브 테스트로 전부 검증 가능 |
| 테스트가 가정을 검증한다 | 오버플로, 제로카피(포인터 비교), 방향(픽셀 색), 메타데이터(재파싱) |

## 다음

- 성능 이야기 → [11. 성능과 메모리](./11-performance-memory.md)
- 마일스톤별 배경 → [../README.md](../README.md)
