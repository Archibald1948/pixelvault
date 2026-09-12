# 05. 컬렉션 · 이터레이터 · 클로저

JS 의 배열 메서드 체인과 거의 같은 감각으로 쓸 수 있는데, **복사가 일어나는 지점이 다르다.**

## 1. 컬렉션 고르기

| 타입 | JS 대응 | 특징 |
|---|---|---|
| `Vec<T>` | `Array` | 소유, 가변 길이, 힙 |
| `&[T]` / `&mut [T]` | (없음) | 빌린 뷰. 복사 없이 자르기 |
| `[T; N]` | (없음) | 길이가 타입에 박힌 고정 배열. 스택 |
| `String` / `&str` | `string` | UTF-8. 인덱싱은 바이트 기준이라 `s[0]` 은 안 된다 |
| `HashMap<K, V>` | `Map`/객체 | 이 레포에선 안 쓴다(필요가 없었다) |
| `Option<T>` | `T \| undefined` | 0개 또는 1개짜리 컨테이너처럼 다룰 수 있다 |

```rust
let mut order: Vec<usize> = (0..n).collect();     // 0..n 범위를 Vec 으로 수집
let mut out = Vec::with_capacity(webp.len() + 64); // 크기를 알면 미리 잡아 재할당을 피한다
out.extend_from_slice(b"RIFF");
out.push(0);
```

`Vec::with_capacity` 는 성능에 직결된다. `push` 로 늘어날 때마다 재할당·복사가 일어나므로,
결과 크기를 대략 알면 미리 잡아 준다(`webp_meta.rs` 에서 그렇게 한다).

## 2. 슬라이스 자르기 — 복사 없는 조작들

```rust
&webp[12..]                         // 12번째부터 끝까지
&body[..size]                       // 처음부터 size 전까지
let (header, body) = rest.split_at(8);        // 두 조각으로
bytes[dst * size..(dst + 1) * size]           // 범위 지정
.copy_from_slice(&original[src * size..(src + 1) * size]);   // 이건 실제 복사(memcpy)
```

- 범위가 길이를 벗어나면 **런타임 panic** 이다. 그래서 `webp_meta.rs` 는 자르기 전에 길이를 검사한다:

```rust
if rest.len() < 8 { return Err(malformed("truncated chunk header")); }
if body.len() < size { return Err(malformed("chunk larger than file")); }
```

- 안전하게 접근하려면 `get(i)` / `get(a..b)` 가 `Option` 을 돌려준다.

## 3. 이터레이터 — 게으르고, 복사가 없다

```rust
// crates/core/src/encode.rs
for (src, dst) in rgba.as_raw().chunks_exact(4).zip(out.chunks_exact_mut(3)) {
    let a = u16::from(src[3]);
    for c in 0..3 {
        dst[c] = ((u16::from(src[c]) * a + 255 * (255 - a) + 127) / 255) as u8;
    }
}
```

- `chunks_exact(4)`: 슬라이스를 4바이트(= RGBA 픽셀)씩 **보는 방법**. 새 배열을 만들지 않는다.
- `chunks_exact_mut(3)`: 쓰기 가능한 버전.
- `zip`: 두 이터레이터를 짝지어 순회. 짧은 쪽에서 멈춘다.
- JS 로 치면 `for (let i = 0; i < n; i++)` 인덱스 계산을 대신해 주면서, **경계 검사가 한 번만** 일어나 더 빠르다.

### 이 레포에 나오는 어댑터들

```rust
.iter()            // &T 로 순회
.iter_mut()        // &mut T 로 순회
.into_iter()       // T 를 꺼내며 순회 (컬렉션 소비)
.enumerate()       // (인덱스, 값)
.map(f)            // 변환
.filter(p)         // 걸러내기
.any(p) / .all(p)  // bool
.sum::<i32>()      // 합계 (타입을 알려 줘야 할 때 터보피시 ::<>)
.collect()         // 다시 컬렉션으로
.min() / .max()
.rev()             // 역순
.flatten()         // 중첩 펼치기
```

실제 사용례:

```rust
// crates/core/src/exif.rs — GPS 태그가 하나라도 있나
let has_gps = exif.fields().any(|f| f.tag.context() == Context::Gps);

// crates/core/src/pipeline.rs 테스트 — 세 채널이 모두 기대값 근처인가
p.0[..3].iter().zip(rgb).all(|(&a, b)| a.abs_diff(b) < 40)

// crates/core/src/blurhash.rs 테스트 — 채널 평균
let avg = |c: usize| px.chunks_exact(4).map(|p| i32::from(p[c])).sum::<i32>() / n;

// crates/core/src/webp_meta.rs 테스트 — FourCC 목록 만들기
parse_chunks(webp).unwrap().iter()
    .map(|c| String::from_utf8_lossy(&c.fourcc).into_owned())
    .collect()
```

**게으르다**: `map`/`filter` 는 그 자리에서 아무 일도 안 한다. `collect`, `for`, `sum` 같은 소비자가 붙어야 실행된다.
그래서 체인을 길게 써도 중간 배열이 생기지 않는다(JS 는 `map().filter()` 마다 새 배열을 만든다).

## 4. 정렬

```rust
// crates/wasm-libc/src/lib.rs
let mut order: Vec<usize> = (0..n).collect();
order.sort_unstable_by(|&a, &b| cmp(elem(a), elem(b)).cmp(&0));
```

- `sort_by(|a, b| ...)`: 비교 함수가 `Ordering`(Less/Equal/Greater)을 돌려준다. JS 의 음수/0/양수와 같은 개념.
- `sort_unstable_by`: 같은 값의 순서를 보장하지 않는 대신 더 빠르다(할당도 없다).
- 실수 정렬은 `sort_by(|a, b| a.partial_cmp(b).unwrap())` — `f64` 는 NaN 때문에 전순서가 아니라서 그렇다.
  (`www/lib/bench.ts` 의 중앙값 계산은 TS 라 그냥 `sort((a,b) => a-b)`)

## 5. 클로저

```rust
|x| x + 1                      // 인자 하나
|&a, &b| ...                   // 패턴으로 바로 역참조
|| some_value                  // 인자 없음
move |x| x + captured          // 캡처한 값의 소유권을 가져감
```

캡처 방식에 따라 세 가지 트레잇이 붙는다:

| 트레잇 | 의미 | 예 |
|---|---|---|
| `Fn` | 빌려서 읽기만. 여러 번 호출 가능 | `map(\|p\| p[0])` |
| `FnMut` | 캡처한 값을 수정 | 카운터 증가 |
| `FnOnce` | 캡처한 값을 소비. 한 번만 | `unwrap_or_else(\|\| expensive())` |

이 레포의 예:

```rust
// crates/wasm-libc/src/lib.rs — original 을 빌려서 i번째 원소 포인터를 만드는 클로저
let elem = |i: usize| original.as_ptr().add(i * size).cast::<c_void>();

// crates/core/src/encode.rs — 에러 변환용 클로저를 변수에 담아 재사용
let unsupported = |e: image::error::UnsupportedError| {
    PixelVaultError::Codec(image::ImageError::Unsupported(e))
};
encoder.set_icc_profile(icc.to_vec()).map_err(unsupported)?;

// crates/core/examples/gen_fixtures.rs — 필드 만들기를 클로저로 짧게
let field = |tag, value| Field { tag, ifd_num: In::PRIMARY, value };
```

### `map_err` 와 `ok_or`

```rust
.map_err(|e| PixelVaultError::WebpEncode(format!("{e:?}")))?   // 에러만 바꾸기
.ok_or(PixelVaultError::UnsupportedInput)?                      // Option → Result
```

## 6. 문자열 다루기

```rust
s.to_ascii_lowercase()          // 소문자 (ASCII 전용, 빠름)
s.as_str()                      // String → &str
"webp".to_owned()               // &str → String
format!("{a}x{b}")              // 조합
String::from_utf8_lossy(&bytes) // 바이트 → 문자열(깨진 건 �로)
exif.starts_with(b"II*\0")      // 바이트 배열 접두사 검사
```

`&str` 은 **UTF-8 바이트 슬라이스**라서 `s[0]` 같은 인덱싱을 못 한다(한 글자가 1~4바이트).
문자 단위가 필요하면 `s.chars()`, 바이트가 필요하면 `s.as_bytes()`.

## 7. 성능 감각

| 연산 | 비용 |
|---|---|
| `&v[a..b]`, `split_at`, `chunks_exact` | 0 (뷰만 만듦) |
| `iter().map().filter()` | 0 (게으름) |
| `.collect()`, `.to_vec()`, `.clone()`, `format!` | 힙 할당 + 복사 |
| `Vec::push` | 보통 0, 가끔 재할당(2배 성장) |
| `Vec::with_capacity(n)` 후 push | 할당 1회 |

이미지 파이프라인처럼 수십 MB 를 다루는 코드에서는 `clone()`/`to_vec()` 이 어디 있는지가 곧 성능이다.
이 레포는 큰 버퍼에 대해 복사가 **JS↔WASM 경계 2회 + 각 단계의 새 버퍼**만 생기도록 설계했다(02 문서의 그림).

## 다음

→ [06. 모듈 · 크레이트 · Cargo](./06-modules-cargo.md)
