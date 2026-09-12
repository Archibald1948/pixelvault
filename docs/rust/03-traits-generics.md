# 03. 타입 · 트레잇 · 제네릭

TS 의 `interface` + 제네릭에 해당하는 부분. 다만 **런타임이 아니라 컴파일 타임에 전부 해결**된다는 점이 다르다.

## 1. 트레잇 = 인터페이스

```rust
// crates/core/src/encode.rs
impl fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
```

`Display` 를 구현하면 `format!("{fmt}")`, `println!("{}", fmt)` 가 가능해진다.

TS 와의 차이: **타입 정의와 구현이 분리**되어 있다. 남이 만든 타입에도 내 트레잇을 구현할 수 있고(고아 규칙 안에서),
내 타입에 표준 트레잇을 나중에 붙일 수도 있다.

이 레포에서 직접 구현한 트레잇:

| 트레잇 | 어디 | 무엇이 가능해지나 |
|---|---|---|
| `Display` | `OutputFormat` | `format!("{fmt}")` |
| `FromStr` | `OutputFormat` | `"webp".parse::<OutputFormat>()` |
| `Default` | `ProcessOptions`, `ExifSummary` | `..Default::default()` |
| `Error` (thiserror 가 생성) | `PixelVaultError` | `?` 로 변환, `{}` 출력 |
| `From<image::ImageError>` (thiserror 가 생성) | `PixelVaultError` | `?` 자동 변환 |

### `FromStr` 예시 — 문자열 경계를 타입으로 좁히기

```rust
impl FromStr for OutputFormat {
    type Err = PixelVaultError;                    // 연관 타입 (TS 의 제네릭 슬롯 비슷)
    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "webp" => Ok(OutputFormat::Webp),
            "jpeg" | "jpg" => Ok(OutputFormat::Jpeg),
            "png" => Ok(OutputFormat::Png),
            other => Err(PixelVaultError::InvalidOption(format!("unknown format {other:?}"))),
        }
    }
}
```

JS 에서 넘어온 `format: string` 을 **한 곳에서** enum 으로 바꾸고, 그 뒤로는 오타가 불가능해진다:

```rust
// crates/wasm/src/lib.rs
format: format.parse()?,   // 타입 추론으로 OutputFormat::from_str 이 불린다
```

## 2. `derive` — 자주 쓰는 구현 자동 생성

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat { #[default] Webp, Jpeg, Png }
```

| derive | 주는 것 |
|---|---|
| `Debug` | `{:?}` 출력. 없으면 `assert_eq!` 실패 메시지도 못 찍는다 |
| `Clone` | `.clone()` |
| `Copy` | 이동 대신 복사 (작고 단순한 타입만) |
| `PartialEq`, `Eq` | `==`, `assert_eq!` |
| `Default` | `Default::default()` |

`Copy` 를 붙일 수 있는 기준: 모든 필드가 `Copy` 이고(힙 자원 없음) 복사가 싸야 한다.
`Metadata<'a> { icc: Option<&'a [u8]>, exif: Option<&'a [u8]> }` 는 참조 두 개뿐이라 `Copy` 다.

## 3. 제네릭과 트레잇 바운드

```rust
// crates/core/src/encode.rs — ImageEncoder 를 구현한 타입이면 무엇이든
fn attach_metadata<E: ImageEncoder>(encoder: &mut E, meta: Metadata<'_>) -> Result<()> {
    if let Some(icc) = meta.icc { encoder.set_icc_profile(icc.to_vec())?; }
    if let Some(exif) = meta.exif { encoder.set_exif_metadata(exif.to_vec())?; }
    Ok(())
}
```

조건이 길어지면 `where` 로 뺀다:

```rust
// crates/core/src/resize.rs
fn resize_typed<P, Px>(src: &ImageBuffer<Px, Vec<u8>>, width: u32, height: u32) -> Result<ImageBuffer<Px, Vec<u8>>>
where
    P: PixelTrait,
    Px: Pixel<Subpixel = u8>,     // 연관 타입에 조건을 걸 수도 있다
{ ... }
```

## 4. 단형화(monomorphization) — 제네릭이 코드 크기에 미치는 영향

TS 제네릭은 컴파일 후 사라진다(타입 검사용). Rust 제네릭은 **쓰인 타입마다 기계어가 따로 생성**된다.

```rust
resize_typed::<U8x3, _>(src, w, h)   // U8x3 전용 함수가 생성됨
resize_typed::<U8x4, _>(src, w, h)   // U8x4 전용 함수가 생성됨
```

- 장점: 가상 호출(동적 디스패치)이 없어 인라이닝·최적화가 잘 된다 → 빠르다.
- 단점: 쓰인 타입 수만큼 코드가 늘어난다(코드 블로트).
- 이 프로젝트에서는 **단점이 장점이 되었다**: 라이브러리에 `DynamicImage`(런타임 분기)를 넘기면
  16비트·float 픽셀 코드와 512 KB 테이블까지 전부 링크된다. 제네릭으로 2개 타입만 쓰게 하니
  나머지가 통째로 사라져서 wasm 이 **1.74 MB → 0.98 MB** 가 됐다. (`docs/m1-core-pipeline.md`)

### 동적 디스패치(`dyn`)와의 차이

```rust
fn f(x: &impl ImageEncoder)     // 정적: 타입마다 사본 생성, 빠름, 코드 큼
fn g(x: &dyn ImageEncoder)      // 동적: 사본 하나, 호출할 때 함수 포인터 조회, 코드 작음
```

이 레포는 전부 정적 디스패치를 쓴다. wasm 크기가 문제가 될 때는 `dyn` 으로 바꾸는 것도 선택지다.

## 5. `impl Trait` — "구체 타입은 신경 쓰지 마"

```rust
let mut decoder = reader.into_decoder()?;   // 반환 타입: impl ImageDecoder + '_
```

반환 위치의 `impl Trait` 는 "이 트레잇을 구현한 **어떤 한 타입**"을 뜻한다.
호출자는 트레잇 메서드만 쓸 수 있고, 라이브러리는 내부 타입을 자유롭게 바꿀 수 있다.

## 6. `From` / `Into` — 변환의 표준 통로

```rust
u64::from(width)               // u32 → u64 (항상 안전)
u8::try_from(v).ok()           // u32 → u8 (실패 가능 → Result)
"webp".to_owned()              // &str → String
icc.to_vec()                   // &[u8] → Vec<u8> (복사)
PixelVaultError::from(e)       // #[from] 이 만들어 준 변환. ? 가 자동으로 부른다
```

`From` 을 구현하면 `Into` 는 공짜로 따라온다. 함수 인자에 `impl Into<String>` 을 받으면
`&str` 과 `String` 을 둘 다 받을 수 있다 — `PixelVaultError::InvalidOption("...".into())` 같은 코드가 그 예다.

## 7. `Deref` 와 자동 변환(coercion)

```rust
let memory = encoder.encode_simple(false, quality)?;   // WebPMemory 타입
Ok(memory.to_vec())                                     // Deref<Target=[u8]> 라서 슬라이스 메서드가 그대로 보인다
```

`Vec<T>` → `&[T]`, `String` → `&str`, `Box<T>` → `&T` 같은 변환이 자동으로 일어나는 것도 `Deref` 덕분이다.

## 8. `Cow` — 빌릴지 소유할지를 런타임에 결정

```rust
// crates/core/src/encode.rs
fn as_rgb8_or_rgba8(image: &DynamicImage) -> Cow<'_, DynamicImage> {
    match image {
        DynamicImage::ImageRgb8(_) | DynamicImage::ImageRgba8(_) => Cow::Borrowed(image),  // 복사 0
        other => Cow::Owned(crate::decode::to_8bit_rgb_or_rgba(other.clone())),            // 변환 필요
    }
}
```

`Cow`(Clone on Write)는 "대부분은 빌려 쓰고, 필요할 때만 새로 만든다"를 **타입으로 표현**한 것이다.
호출하는 쪽은 `&*image` 처럼 그냥 참조로 쓰면 되고, 실제로 복사가 일어났는지는 신경 쓰지 않아도 된다.

## 9. `Drop` — 소멸자

스코프를 벗어날 때 자동으로 불리는 정리 코드. 직접 구현할 일은 드물지만, **쓰고 있다**:

```rust
let memory = encoder.encode_simple(false, f32::from(quality))?;  // libwebp 가 malloc 한 버퍼를 감싼 타입
Ok(memory.to_vec())   // Rust Vec 으로 복사
// 함수 끝 → memory 의 Drop 이 불려서 C 쪽 WebPFree 가 실행된다
```

C 라이브러리의 자원을 Rust 타입으로 감싸면, **해제를 잊을 수가 없다**. RAII 라고 부르는 패턴이다.

## 10. 마커 트레잇: `Send`, `Sync`

"스레드 간에 옮겨도/공유해도 되는 타입"을 뜻한다. wasm32 는 단일 스레드라 이 레포에서 직접 신경 쓸 일은 없지만,
네이티브에서 `rayon` 같은 병렬 처리를 붙이면 바로 만나게 된다. (이 프로젝트의 병렬화는 Rust 스레드가 아니라
**워커 여러 개**로 했다 — `docs/m3-worker.md`)

## 다음

→ [04. 에러 처리](./04-error-handling.md)
