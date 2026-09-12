# 04. 에러 처리 — `Option`, `Result`, `?`, thiserror

Rust 에는 `null` 도 `throw` 도 없다. 둘 다 **타입**으로 표현한다.

## 1. `Option<T>` — 값이 없을 수 있음

```rust
pub enum Option<T> { Some(T), None }
```

```rust
pub struct Decoded {
    pub icc: Option<Vec<u8>>,     // ICC 프로파일이 없을 수도 있다
    pub exif: Option<Vec<u8>>,
}
pub max_width: Option<u32>,       // "제한 없음"을 None 으로
```

TS 의 `Vec<u8> | undefined` 와 같지만, **처리하지 않으면 컴파일이 안 된다.**
`opt.len()` 같은 건 못 쓴다. 먼저 꺼내야 한다.

### 꺼내는 방법들

```rust
match opt { Some(v) => ..., None => ... }        // 가장 명시적
if let Some(icc) = meta.icc { ... }              // 한 가지 경우만
let Some(layout) = layout_for(size) else { return null_mut(); };   // 꺼내거나 탈출
opt.unwrap()                                      // None 이면 panic (테스트에서만)
opt.expect("fixture has EXIF")                    // panic + 메시지 (테스트/불변식)
opt.unwrap_or(1)                                  // 기본값
opt.unwrap_or_default()                           // 타입의 기본값
```

### 변환하는 방법들 (이 레포에 실제로 나온 것)

```rust
// crates/core/src/exif.rs
exif.get_field(Tag::Orientation, In::PRIMARY)     // Option<&Field>
    .and_then(|field| field.value.get_uint(0))    // Option<u32>   (map + flatten)
    .and_then(|v| u8::try_from(v).ok())           // Option<u8>    (Result → Option 은 .ok())
    .filter(|v| (1..=8).contains(v))              // 조건 불만족이면 None
    .unwrap_or(1);                                // 최종 기본값

// crates/core/src/decode.rs — 에러를 "정보 없음"으로 흡수
let icc = decoder.icc_profile().ok().flatten();   // Result<Option<T>> → Option<T>

// crates/core/src/pipeline.rs
options.blurhash
    .then(|| crate::blurhash::encode(&image))     // bool → Option<Result<String>>
    .transpose()?;                                 // → Result<Option<String>> 로 뒤집고 ? 로 전파
```

`and_then` 은 JS 의 옵셔널 체이닝(`a?.b?.c`)에 해당하고, `map` 은 값만 바꾼다.
**`?` 연산자도 `Option` 에 쓸 수 있다**(함수 반환 타입이 `Option` 일 때):

```rust
fn layout_for(user_size: usize) -> Option<Layout> {
    let total = user_size.checked_add(HEADER)?;   // 오버플로면 None 을 즉시 반환
    Layout::from_size_align(total, HEADER).ok()
}
```

## 2. `Result<T, E>` — 실패할 수 있음

```rust
pub enum Result<T, E> { Ok(T), Err(E) }
```

```rust
// crates/core/src/error.rs
pub type Result<T> = std::result::Result<T, PixelVaultError>;   // 에러 타입을 고정한 별칭
```

함수 시그니처만 봐도 실패 가능성이 드러난다:

```rust
pub fn process(input: &[u8], options: &ProcessOptions) -> Result<ProcessOutput>
pub fn fit_within(...) -> (u32, u32)        // 실패할 수 없는 함수는 Result 가 없다
```

TS 는 `function process(): Output` 만 보고는 throw 하는지 알 수 없다. Rust 는 타입에 적혀 있다.

## 3. `?` 연산자

```rust
let decoded = decode(input)?;
// 아래와 같다:
let decoded = match decode(input) {
    Ok(v) => v,
    Err(e) => return Err(From::from(e)),   // 에러 타입 변환까지 해 준다
};
```

`?` 는 성공 값을 꺼내고, 실패면 **즉시 반환**한다. `try/catch` 없이 에러가 호출 스택을 타고 올라간다.
차이점은 **어디서 멈추는지가 코드에 보인다**는 것: `?` 가 붙은 줄이 곧 "여기서 실패할 수 있음" 표시다.

## 4. 에러 타입 설계: `thiserror`

```rust
#[derive(Debug, thiserror::Error)]
pub enum PixelVaultError {
    #[error("unsupported input format (supported: JPEG, PNG, WebP)")]
    UnsupportedInput,

    #[error("image too large: {width}x{height} would need {needed_bytes} bytes decoded (limit {limit_bytes})")]
    TooLarge { width: u32, height: u32, needed_bytes: u64, limit_bytes: u64 },

    #[error("invalid option: {0}")]
    InvalidOption(String),

    #[error("image codec error: {0}")]
    Codec(#[from] image::ImageError),        // From 구현 자동 생성 → ? 가 자동 변환

    #[error("jpeg encode failed: {0}")]
    JpegEncode(#[from] jpeg_encoder::EncodingError),

    #[error("webp encode failed: {0}")]
    WebpEncode(String),                       // 남의 에러 타입이 Error 를 구현 안 하면 문자열로
}
```

- `#[error("...")]` → `Display` 구현. `{width}` 로 필드를, `{0}` 으로 튜플 필드를 참조한다.
- `#[from]` → `impl From<image::ImageError> for PixelVaultError` 생성. 이게 있어야 `?` 가 알아서 감싼다.
- **에러에 정보를 담자**: `TooLarge` 는 크기와 한도를 같이 들고 있어서, UI 가 "몇 픽셀이라 거절됐는지" 보여줄 수 있다.

### 왜 enum 인가

호출자가 `match` 로 **경우별로 다르게 대응**할 수 있다. 테스트도 종류를 콕 집어서 검사한다:

```rust
assert!(matches!(err, PixelVaultError::TooLarge { width: 640, height: 480, .. }));
```

## 5. `panic!` 은 언제 쓰나

`panic!` 은 복구 불가능한 상황에서 프로세스를 중단시킨다(wasm 에서는 트랩 → JS 예외).

| 상황 | 선택 |
|---|---|
| 사용자가 준 입력이 잘못됨 | `Result` (에러로 알린다) |
| 라이브러리 사용법이 잘못됨(우리 버그) | `panic!` / `unreachable!` / `assert!` |
| 테스트 | `unwrap()`, `assert_eq!` 마음껏 |

```rust
// crates/core/src/encode.rs — 여기 오면 우리 코드의 순서가 잘못된 것
_ => unreachable!("as_rgb8_or_rgba8 guarantees RGB8/RGBA8"),
```

라이브러리 코드에서 `unwrap()` 을 쓰려면 "여기서 절대 None 이 아닌 이유"를 주석이나 `expect` 메시지로 남기자:

```rust
let fourcc: [u8; 4] = header[0..4].try_into().expect("slice of len 4");
```

## 6. 관대하게 처리하기 vs 엄격하게 처리하기

같은 실패라도 대응이 다르다. 이 레포의 기준:

```rust
// 메타데이터가 깨졌다고 이미지 변환 전체를 실패시키지 않는다 → None 으로 흡수
let icc = decoder.icc_profile().ok().flatten();
let Ok(exif) = Reader::new().read_raw(raw_exif.to_vec()) else {
    return ExifSummary::default();
};

// 반면 이미지 자체가 잘못됐거나 메모리 한도를 넘으면 → 에러로 올린다
check_decoded_size(width, height, max_decoded_bytes)?;
```

"이 실패가 사용자가 원한 결과를 못 주게 하는가?"를 기준으로 나눈다.

## 7. WASM 경계까지 에러 전달하기

```rust
// crates/wasm/src/lib.rs
pub fn process_image_raw(...) -> Result<RawProcessResult, JsError> {
    let options = ProcessOptions { format: format.parse()?, ... };   // PixelVaultError → JsError
    let out = pixelvault_core::process(input, &options)?;
    Ok(RawProcessResult { ... })
}
```

`JsError` 는 `std::error::Error` 를 구현한 **모든 타입**에서 `From` 변환을 제공한다.
그래서 `?` 한 글자로 변환이 끝나고, JS 쪽에서는 평범한 예외로 잡힌다:

```ts
try {
  wasm.processImageRaw(bytes, ...);
} catch (e) {
  console.error(e.message);   // "invalid option: quality must be 1-100, got 0"
}
```

`Display` 로 만든 문자열이 그대로 `e.message` 가 되므로, **에러 메시지를 사람이 읽을 수 있게 쓰는 게 중요**하다.

## 8. 정리

```
없을 수 있다        → Option<T>   → match / if let / ? / unwrap_or
실패할 수 있다      → Result<T,E> → ?  (에러 타입은 thiserror enum)
우리 코드의 버그다  → panic! / unreachable! / assert!
남의 에러를 내 에러로 → #[from] 또는 map_err
JS 로 보낸다        → Result<T, JsError>, ? 가 알아서 변환
```

## 다음

→ [05. 컬렉션 · 이터레이터 · 클로저](./05-collections-iterators.md)
