# M2 — EXIF & BlurHash

> 목표: ① 아이폰 세로 사진이 눕지 않게, ② GPS 같은 개인정보는 기본적으로 지우고, ③ 로딩용 BlurHash 를 만든다.

## 새로 추가된 파일

| 파일 | 역할 |
|---|---|
| `crates/core/src/exif.rs` | EXIF 요약(Orientation, GPS 유무), 방향 보정, Orientation 태그 초기화 |
| `crates/core/src/blurhash.rs` | BlurHash 인코딩(32px 로 줄여서 계산) / 디코딩 |
| `crates/core/src/webp_meta.rs` | WebP RIFF 컨테이너에 ICC/EXIF 청크 붙이기 |
| `www/components/BlurhashCanvas.tsx` | BlurHash → 캔버스 플레이스홀더 |

## 1. "사진이 눕는" 버그의 정체

아이폰 카메라 센서는 항상 가로로 누워 있다. 세로로 들고 찍어도 픽셀은 **가로(4032×3024)로 저장**하고,
EXIF 에 `Orientation = 6` ("보여줄 때 시계방향 90° 돌려라") 이라는 **메모만** 붙인다.

```
저장된 픽셀(가로)         EXIF: Orientation=6         화면에 보이는 모습(세로)
┌───────────────┐                                     ┌─────────┐
│ ☀️            │ ── 뷰어가 시계방향 90° 돌려서 ──▶   │☀️       │
│    (누워 있음) │         보여줌                      │         │
└───────────────┘                                     │  (세로)  │
                                                      └─────────┘
```

이미지 처리 도구가 픽셀만 다시 인코딩하면서 EXIF 를 버리면 → 메모가 사라짐 → **누운 사진**.
실무에서 가장 흔한 이미지 업로드 버그 중 하나다.

### 해결: EXIF 를 지우기 전에 회전을 픽셀에 반영

```rust
let summary = exif.as_deref().map(exif::summarize).unwrap_or_default(); // Orientation 읽기 (kamadak-exif)
...
let image = exif::apply_orientation(image, summary.orientation);         // 픽셀을 실제로 돌림
```

Orientation 8가지:

| 값 | 의미 | 흔한 상황 |
|---|---|---|
| 1 | 그대로 | 대부분의 사진 |
| 3 | 180° | 폰을 거꾸로 들고 찍음 |
| **6** | **시계방향 90°** | **아이폰 세로 사진** |
| 8 | 반시계 90° | 폰을 반대로 눕혀서 세로 |
| 2, 4, 5, 7 | 좌우/상하 반전 포함 | 셀카 미러링 등 (드묾) |

### 최적화: 리사이즈 먼저, 회전은 나중에

회전은 픽셀 전체를 새 버퍼로 복사하는 작업이다. 12MP 원본(RGB 36 MB)을 돌리는 것보다
1920px 로 줄인 결과(8 MB)를 돌리는 게 4.5배 싸다. 90° 회전과 리사이즈는 **순서를 바꿔도 결과가 같으므로**:

```rust
// maxWidth/maxHeight 는 "화면 기준". 90° 돌려야 하는 사진은 저장된 가로 = 화면 세로이므로 뒤바꿔서 리사이즈.
let (max_w, max_h) = if exif::swaps_dimensions(summary.orientation) {
    (options.max_height, options.max_width)
} else {
    (options.max_width, options.max_height)
};
let image = resize(image, max_w, max_h)?;              // 먼저 줄이고
let image = exif::apply_orientation(image, summary.orientation); // 작은 걸 돌린다
```

`max_width_applies_to_displayed_orientation` 테스트가 이 뒤바꿈을 검증한다 (maxWidth=150 → 150×200, 저장 기준으로 잘못 계산했다면 150×113).

## 2. EXIF 제거 (기본값) — 그리고 "유지"할 때의 함정

### 제거는 공짜다

픽셀을 새로 인코딩하면 원본의 메타데이터는 **아무것도 안 하면 자동으로 사라진다**. 인코더에 EXIF 를 넘기지 않으면 끝.
`gps_is_removed_by_default` 테스트는 WebP/JPEG/PNG 결과물을 `kamadak-exif` 로 다시 열어서 EXIF 가 **아예 없는지** 확인한다.

### `stripExif: false` 로 유지할 때

사용자가 촬영 정보를 남기고 싶어할 수도 있다. 이때 원본 EXIF 를 그대로 붙이면 **이중 회전** 버그가 생긴다:
픽셀은 이미 돌렸는데 Orientation=6 이 남아 있으니 뷰어가 한 번 더 돌린다.

```rust
exif.map(|mut raw| {
    exif::reset_orientation(&mut raw);  // Orientation 태그 2바이트만 1 로 고쳐 쓴다
    raw
})
```

`reset_orientation(raw_exif: &mut [u8])` — **가변 슬라이스** 로 받아서 제자리에서 고친다. 새 버퍼를 만들지 않는다.
(`&[u8]` = 읽기 전용 빌림, `&mut [u8]` = 쓰기 가능한 빌림. 동시에 하나만 존재할 수 있다 — Rust 의 핵심 규칙)

### ICC 색 프로파일은 항상 유지

EXIF 와 비슷하게 파일에 붙어 다니는 **ICC 프로파일** 은 "이 RGB 숫자를 어떤 색 공간으로 해석하라"는 정보다.
아이폰 사진은 sRGB 보다 넓은 **Display P3** 이다. 프로파일을 버리면 브라우저가 sRGB 로 해석해서 **색이 칙칙해진다.**
개인정보가 아니므로 `stripExif` 와 무관하게 항상 결과에 넣는다. (`icc_profile_is_preserved_in_every_format` 테스트)

## 3. WebP 에 메타데이터 넣기 — 바이트 조립 연습

`image` 크레이트의 JPEG/PNG 인코더는 `set_exif_metadata()`, `set_icc_profile()` 을 지원한다.
하지만 libwebp 의 간단 API 는 메타데이터를 받지 않는다. 다행히 WebP 파일 구조는 단순하다:

```
RIFF <크기> WEBP  [VP8X 10바이트: 플래그 + 캔버스 크기]  [ICCP …]  [ALPH …]  [VP8 …]  [EXIF …]
```

`webp_meta.rs` 는 libwebp 결과를 청크 단위로 쪼개고(`split_at`), VP8X 헤더를 새로 써서 다시 조립한다.
여기서 볼 Rust 포인트:

- **라이프타임 `'a`**: `struct Chunk<'a> { payload: &'a [u8] }` — 청크는 원본 버퍼를 빌려서 가리키기만 한다.
  원본이 해제된 뒤 청크를 쓰는 코드는 **컴파일이 안 된다**. C 였다면 dangling pointer 버그.
- **`u32::from_le_bytes` / `to_le_bytes`**: 리틀엔디언 정수 ↔ 바이트 배열. 파일 포맷 파싱의 기본기.
- **`try_into()`**: `&[u8]` (길이 모름) → `[u8; 4]` (길이 4 고정) 변환. 길이가 다르면 실패하므로 `Result` 를 돌려준다.
- 메타데이터가 없으면 `Vec<u8>` 을 **그대로 돌려준다** — 입력을 값으로 받았기 때문에 가능한 복사 0 경로.

## 4. BlurHash

```
LEHV6nWB2yk8pyo0adR*.7kCMdnj  ← 28글자로 이미지의 대략적인 색 분포를 표현
```

- 이미지를 몇 개(4×3)의 코사인 성분으로 근사해서 base83 으로 인코딩한다. JPEG 의 DCT 와 비슷한 원리.
- 서버 DB 에 이 문자열만 저장해 두면 진짜 이미지가 오기 전에 **색감이 맞는 흐린 플레이스홀더** 를 바로 그릴 수 있다.

### 32px 로 줄여서 계산

인코딩 비용은 `가로 × 세로 × 성분 수` 에 비례한다. 결과가 어차피 성분 12개뿐이라 32px 로 줄여도 문자열은 사실상 같다.
여기서 `resize` 와 `resized_copy` 의 차이가 등장한다:

```rust
pub fn resize(image: DynamicImage, ...)       // 소유권을 가져감 = "원본은 이제 필요 없음"
pub fn resized_copy(image: &DynamicImage, ...) // 빌리기만 함   = "원본도 계속 쓸 거니까 사본을 만들어 줘"
```

BlurHash 는 인코딩할 원본을 건드리면 안 되므로 `resized_copy(&image)` 를 쓴다.
처음에는 `image.clone()` 후 `resize` 했는데, 그러면 1920px 이미지 전체(8 MB)를 쓸데없이 한 번 더 복사한다.

### 가로/세로에 따라 성분 배분

가로로 긴 사진은 가로 성분 4 × 세로 3, 세로 사진은 3 × 4. 긴 쪽에 해상도를 더 준다.

### 테스트에서 배운 것

처음엔 "단색 이미지의 BlurHash 를 풀면 모든 픽셀이 원래 색"이라고 테스트를 짰는데 실패했다(200 → 218).
조사해 보니 BlurHash 는 AC 성분을 19단계로 양자화하는데 **가장 작은 단계도 0 이 아니어서**, 단색에도 약한 무늬가 생긴다.
평균색(DC 성분)은 정확히 보존된다 → 테스트를 "평균색 비교"로 수정. 테스트가 틀린 가정을 알려준 사례.

## 5. `Option` 다루기 패턴 모음 (이번 마일스톤에 많이 나옴)

```rust
exif.as_deref()                 // Option<Vec<u8>> → Option<&[u8]>  (안을 빌려 보기)
    .map(exif::summarize)       // Some(x) 면 함수 적용, None 이면 그대로 None
    .unwrap_or_default();       // None 이면 Default::default()

options.blurhash                // bool
    .then(|| blurhash::encode(&image))  // true 면 Some(결과), false 면 None → Option<Result<String>>
    .transpose()?;              // Option<Result<T>> → Result<Option<T>> 로 뒤집고 ? 로 에러 전파

kept_exif.filter(|e| e.len() <= JPEG_MAX_EXIF)  // 조건 안 맞으면 None 으로
```

JS 의 `?.` / `??` 와 비슷하지만, `None` 을 처리하지 않으면 **컴파일이 안 된다** 는 점이 다르다.

## 6. WASM 경계 추가 사항

- `Option<String>` getter → JS 에서 `string | undefined`
- `decodeBlurhash(hash, w, h)` → `Uint8Array`(RGBA). 캔버스에 `new ImageData(...)` 로 그린다.
- 새 결과 필드: `sourceOrientation`, `hadGps`, `exifKept` — UI 에서 "↻ 방향 보정됨", "🛰 GPS 위치 정보 제거됨" 배지로 표시.

## 테스트 요약

- core: 48개 (`cargo test`) — 그중 M2 관련:
  - `iphone_portrait_is_not_lying_down` — 3개 포맷 모두 300×400, 해가 왼쪽 위 / 땅이 아래 (픽셀 색으로 검증)
  - `gps_is_removed_by_default` — 3개 포맷 결과에 EXIF 자체가 없음
  - `keeping_exif_resets_orientation_to_avoid_double_rotation`
  - `icc_profile_is_preserved_in_every_format`
  - `webp_meta::*` — 청크 순서, 플래그, RIFF 크기, 홀수 패딩
- wasm: 4개 (`./scripts/test-wasm.sh`) — `iphone_portrait_in_wasm` 추가

픽스처 `iphone-portrait.jpg` 는 `kamadak-exif` 의 `Writer` 로 Orientation=6 + 서울 GPS 좌표를 넣어 생성했다 (`examples/gen_fixtures.rs`).

## wasm 크기

M1 378 KB → M2 **403 KB** (gzip). kamadak-exif + blurhash + WebP 컨테이너 코드 추가분 25 KB.
