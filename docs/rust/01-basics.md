# 01. 기초 문법 — TS 개발자를 위한 Rust 입문

이 레포 코드에 나오는 문법만 추린다. 예제는 전부 실제 파일에서 가져왔다.

## 1. 변수: 기본이 불변(immutable)

```rust
let format = OutputFormat::Webp;      // 못 바꿈 (TS 의 const)
let mut out = Vec::new();             // 바꿀 수 있음 (TS 의 let)
out.extend_from_slice(b"RIFF");       // mut 이 없으면 여기서 컴파일 에러
```

- `mut` 는 "이 변수를 통해 값을 바꿀 수 있다"는 뜻. 안 쓰면 컴파일러가 "mut 필요 없음" 경고를 준다.
- **섀도잉(shadowing)**: 같은 이름으로 다시 선언할 수 있다. 값을 단계적으로 변환할 때 자주 쓴다.

```rust
// crates/core/src/pipeline.rs
let image = resize(image, max_w, max_h)?;              // 앞의 image 를 가리고 새 image
let image = exif::apply_orientation(image, orientation); // 또 가림
```

TS 에서 `image1`, `image2`, `image3` 로 번호를 붙이던 상황을, Rust 는 같은 이름으로 덮어쓰며 처리한다.
이전 값은 더 이상 접근할 수 없으므로 "실수로 예전 값을 쓰는" 버그가 막힌다.

- `const`: 컴파일 타임 상수. 타입을 반드시 적는다.

```rust
// crates/core/src/decode.rs
pub const DEFAULT_MAX_DECODED_BYTES: u64 = 1 << 30;   // 1 GiB
```

## 2. 타입

### 숫자

| 타입 | 범위 | 이 레포에서 |
|---|---|---|
| `u8` | 0~255 | 픽셀 한 채널, quality(1~100) |
| `u16` | 0~65535 | JPEG 가로/세로 |
| `u32` | 0~42억 | 이미지 가로·세로, 좌표 |
| `u64` | 아주 큼 | 바이트 수 계산(오버플로 방지용) |
| `usize` | 포인터 크기 (wasm32 = 32비트, 맥 = 64비트) | 배열 인덱스, 길이 |
| `f32`/`f64` | 실수 | 비율 계산, 품질 |

JS 의 `number` 하나가 전부 이 타입들로 쪼개진다. **자동 변환이 없다**:

```rust
let w: u32 = 1920;
let scale = w as f64 / 4032.0;         // as = 명시적 변환
let bytes = u64::from(w) * 4;          // 안전한 확대 변환 (u32 -> u64)
let small = u8::try_from(value).ok();  // 줄이는 변환은 실패할 수 있어서 Result
```

`as` 는 **말없이 잘라내므로**(300u32 as u8 == 44) 크기 검사 같은 곳에서는 `u64::from` / `try_from` 을 쓴다.

### 복합 타입

```rust
let (w, h) = fit_within(4032, 3024, Some(1920), None);   // 튜플 (TS 와 동일)
let fourcc: [u8; 4] = *b"VP8X";                          // 고정 길이 배열 (길이가 타입의 일부)
let mut chunks: Vec<Chunk> = Vec::new();                 // 가변 길이 벡터 (TS 의 Array)
let view: &[u8] = &buffer[12..];                         // 슬라이스 = 빌린 뷰 (02 문서 참고)
```

### 문자열: `String` 과 `&str`

```rust
pub fn as_str(self) -> &'static str { "webp" }     // &str: 어딘가에 있는 문자열을 빌린 것
self.format.as_str().to_owned()                     // String: 소유한 문자열 (힙 할당)
format!("quality must be 1-100, got {quality}")     // format! = TS 의 템플릿 리터럴
```

- `"webp"` 같은 리터럴은 `&'static str` — 실행 파일 안에 박혀 있고 프로그램 내내 산다.
- 함수 인자는 보통 `&str` 로 받고(더 유연), 저장할 때 `String` 으로 만든다.
- `b"RIFF"` 는 문자열이 아니라 **바이트 배열** `&[u8; 4]`. 파일 포맷 다룰 때 쓴다.

## 3. 함수

```rust
// crates/core/src/resize.rs
pub fn fit_within(width: u32, height: u32, max_width: Option<u32>, max_height: Option<u32>) -> (u32, u32) {
    let scale = ...;
    if scale >= 1.0 {
        return (width, height);   // 이른 반환은 return
    }
    (w, h)                        // 마지막 표현식이 반환값 (세미콜론 없음!)
}
```

- `pub` 가 없으면 **모듈 밖에서 안 보인다**(기본이 비공개).
- 반환 타입은 `->` 뒤에. 반환값이 없으면 생략(`()`, 유닛 타입).
- **세미콜론이 없는 마지막 줄이 반환값**이다. 실수로 `;` 를 붙이면 "타입이 `()` 라서 안 맞는다"는 에러가 난다.

Rust 는 **표현식 지향** 언어라 `if`, `match`, 블록이 모두 값을 만든다:

```rust
let (cx, cy) = if width >= height { (4, 3) } else { (3, 4) };
```

## 4. 제어문

```rust
// if 는 조건에 괄호가 없고, 조건은 반드시 bool
if w > WEBP_MAX_DIMENSION || h > WEBP_MAX_DIMENSION { ... }

// for 는 이터레이터를 순회한다 (C 스타일 for 는 없다)
for (dst, &src) in order.iter().enumerate() { ... }
for y in 0..16 { ... }            // 0..16 = 0 이상 16 미만, 0..=16 = 16 포함

// while
while lo < hi { ... }

// loop = 무한 루프, break 로 값을 낼 수도 있다
loop { break; }
```

### `match` — 강력한 switch

```rust
// crates/core/src/encode.rs
match format {
    OutputFormat::Webp => encode_webp(&image, quality),
    OutputFormat::Jpeg => encode_jpeg(&image, quality, meta),
    OutputFormat::Png => encode_png(&image, meta),
}
```

- **모든 경우를 처리했는지 컴파일러가 검사한다**(exhaustive). 하나라도 빠지면 컴파일 에러.
  → enum 에 변형을 추가하면 처리 안 한 곳이 전부 에러로 드러난다. 리팩터링이 안전해지는 핵심 장치.
- 패턴에는 값·범위·구조 분해를 쓸 수 있다:

```rust
// crates/core/src/decode.rs — 여러 값을 한 팔로 묶고, 매치된 값을 f 로 받는다
let format = match reader.format() {
    Some(f @ (ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::WebP)) => f,
    _ => return Err(PixelVaultError::UnsupportedInput),   // _ = 나머지 전부
};

// crates/core/src/webp_meta.rs — 바이트 배열 패턴
match &chunk.fourcc {
    b"VP8X" => { ... }
    b"ICCP" | b"EXIF" | b"XMP " => {}    // 아무것도 안 함
    _ => image_chunks.push(*chunk),
}

// crates/core/src/exif.rs — 범위 패턴
matches!(orientation, 5..=8)
```

- `matches!(값, 패턴)` 은 "이 패턴에 맞나?"를 bool 로 주는 매크로. 테스트에서 자주 쓴다.
- `if let` 은 한 가지 경우만 볼 때:

```rust
if let Some(icc) = meta.icc {
    encoder.add_icc_profile(icc)?;
}
```

- `let ... else` 는 "꺼내거나, 안 되면 탈출":

```rust
let Some(layout) = layout_for(size) else {
    return core::ptr::null_mut();
};
```

## 5. 구조체(struct)

```rust
// crates/core/src/decode.rs
#[derive(Debug)]                 // Debug 출력 구현을 자동 생성
pub struct Decoded {
    pub image: DynamicImage,     // 필드도 각각 pub 를 붙여야 밖에서 보인다
    pub format: ImageFormat,
    pub icc: Option<Vec<u8>>,
    pub exif: Option<Vec<u8>>,
}

// 만들기
Decoded { image, format, icc, exif }     // 변수명과 필드명이 같으면 생략 가능(TS 와 동일)

// 구조 분해
let Decoded { image, format: source_format, icc, exif } = decode(input)?;
```

메서드는 `impl` 블록에 따로 쓴다:

```rust
// crates/core/src/encode.rs
impl OutputFormat {
    pub fn as_str(self) -> &'static str { ... }        // self = 메서드 (obj.as_str())
    pub fn mime_type(self) -> &'static str { ... }
}
```

- `self` / `&self` / `&mut self` 로 "이 메서드가 값을 가져가는지, 빌리는지, 바꾸는지"를 표시한다(02 문서).
- `self` 가 없는 함수는 **연관 함수**(TS 의 static). 생성자 관례가 `new`: `Resizer::new()`.

### 기본값 채우기: `..Default::default()`

```rust
// 테스트에서 자주 보이는 패턴
ProcessOptions { max_width: Some(320), ..Default::default() }
```

TS 의 `{ ...defaults, maxWidth: 320 }` 와 같은데, 순서가 반대(나머지를 뒤에 적는다)다.

## 6. enum — 값을 품는 유니언

```rust
// crates/core/src/encode.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    #[default] Webp,      // 단순 변형
    Jpeg,
    Png,
}

// crates/core/src/error.rs — 변형이 데이터를 품는다
pub enum PixelVaultError {
    UnsupportedInput,                                   // 데이터 없음
    InvalidOption(String),                              // 튜플형
    TooLarge { width: u32, height: u32, ... },          // 구조체형
    Codec(#[from] image::ImageError),
}
```

TS 의 discriminated union 과 같은 개념인데, **런타임 태그가 언어에 내장**되어 있고 `match` 로 안전하게 꺼낸다.
표준 라이브러리의 `Option<T>`(Some/None)과 `Result<T, E>`(Ok/Err)도 그냥 enum 이다(04 문서).

## 7. 매크로: 이름 뒤의 `!`

`println!`, `format!`, `vec!`, `matches!`, `assert_eq!`, `include_bytes!` 처럼 `!` 가 붙으면 매크로다.
함수로는 못 하는 일(가변 인자, 컴파일 타임 처리)을 한다.

```rust
const JPEG: &[u8] = include_bytes!("../tests/fixtures/landscape.jpg");  // 컴파일 시점에 파일을 박아 넣음
let msg = format!("{width}x{height}");                                  // 문자열 보간
assert_eq!(out.dimensions(), (320, 240));                               // 테스트
unreachable!("as_rgb8_or_rgba8 guarantees RGB8/RGBA8");                 // 여기 오면 버그 → panic
```

### 포맷 문법

```rust
format!("{}", x)          // Display (사람용)
format!("{:?}", x)        // Debug (개발자용, #[derive(Debug)] 필요)
format!("{x}")            // 변수 이름을 그대로 (Rust 2021+)
format!("{:.1}", 3.14159) // 소수점 1자리
format!("{e:?}")          // 변수 + Debug
```

## 8. 속성(attribute) `#[...]`

TS 데코레이터와 비슷한 위치의 메타데이터다.

```rust
#[derive(Debug, Clone, Copy)]     // 트레잇 구현 자동 생성
#[cfg(test)]                      // 테스트 빌드에서만 컴파일
#[cfg(target_arch = "wasm32")]    // wasm 빌드에서만
#[wasm_bindgen]                   // JS 에 노출 (08 문서)
#[test]                           // 테스트 함수
#[unsafe(no_mangle)]              // 심볼 이름을 그대로 유지 (07 문서)
#![cfg(...)]                      // ! 가 붙으면 "이 파일/크레이트 전체에" 적용
```

## 9. 주석과 문서 주석

```rust
// 일반 주석
/// 이 아래 항목에 대한 문서 (cargo doc, IDE 툴팁에 나온다)
//! 이 파일/모듈 자체에 대한 문서 (파일 맨 위에 쓴다)
```

이 레포는 모든 파일 맨 위에 `//!` 로 "이 파일이 무슨 역할인지"를 적어 두었다. `cargo doc --open` 으로 볼 수 있다.

## 다음

→ [02. 소유권 · 빌림 · 라이프타임](./02-ownership.md) — Rust 가 다른 언어와 결정적으로 다른 부분.
