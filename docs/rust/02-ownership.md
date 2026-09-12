# 02. 소유권 · 빌림 · 라이프타임

Rust 가 GC 없이도 메모리 안전한 이유. 이 장만 이해하면 나머지는 문법 문제다.

## 1. 왜 필요한가

JS 는 GC 가 "아무도 안 쓰는 값"을 찾아서 치운다. C 는 `free()` 를 직접 부른다(실수하면 크래시).
Rust 는 제3의 길을 택했다: **컴파일러가 "여기서 값의 수명이 끝난다"를 계산해서 해제 코드를 넣는다.**
런타임 비용이 0 이고, 이중 해제·해제 후 사용 같은 버그가 컴파일 단계에서 막힌다.

그 계산이 가능하려면 규칙이 필요하다:

> 1. 모든 값에는 **주인(owner)이 정확히 하나** 있다.
> 2. 주인이 스코프를 벗어나면 값은 **버려진다(drop)**.
> 3. 값을 넘기면 주인이 **바뀐다(move)**. 이전 주인은 더 이상 못 쓴다.

## 2. 이동(move)

```rust
let decoded = decode(input)?;                    // decoded 가 픽셀 버퍼의 주인
let image = resize(decoded.image, w, h)?;        // 주인이 resize 안으로 이동
// println!("{}", decoded.image.width());        // ❌ error[E0382]: borrow of moved value
```

에러 메시지는 이렇게 나온다:

```
error[E0382]: borrow of moved value: `decoded.image`
   |
   |     let image = resize(decoded.image, w, h)?;
   |                        ------------- value moved here
   |     decoded.image.width();
   |     ^^^^^^^^^^^^^ value borrowed here after move
```

**이게 왜 좋은가** — `crates/core/src/resize.rs` 를 보자:

```rust
pub fn resize(image: DynamicImage, max_width: Option<u32>, max_height: Option<u32>) -> Result<DynamicImage> {
    let (w, h) = fit_within(image.width(), image.height(), max_width, max_height);
    if (w, h) == (image.width(), image.height()) {
        return Ok(image);          // 그대로 돌려줌 = 복사 0회
    }
    resized_copy(&image, max_width, max_height)
    // 함수가 끝나면서 원본 image 가 drop → 36MB 버퍼가 여기서 해제된다
}
```

12MP 사진의 원본 픽셀은 36 MB. 리사이즈 결과(8 MB)를 만든 직후 원본을 해제할 수 있는 이유는,
**호출한 쪽이 원본을 더 이상 쓸 수 없다는 걸 컴파일러가 보장**하기 때문이다.
JS 였다면 `decoded` 변수가 스코프에 살아 있는 한 GC 가 치우지 못한다.

### Copy 타입은 예외

정수·bool·`char`·고정 크기 배열 등 "복사가 싼" 타입은 이동 대신 **복사**된다.

```rust
let q: u8 = 82;
let q2 = q;        // 복사. q 도 계속 쓸 수 있다
```

`#[derive(Clone, Copy)]` 를 붙인 우리 타입도 마찬가지다(`OutputFormat`, `ExifSummary`, `Metadata`).
반대로 `Vec`, `String`, `DynamicImage` 처럼 힙을 쓰는 타입은 이동한다.

### 명시적 복사: `clone()`

```rust
other.clone()     // 깊은 복사. 비용이 드는 게 코드에 드러난다
```

JS 는 `structuredClone` 이 명시적이지만 객체 대입은 참조 공유라 헷갈린다. Rust 는 **복사하려면 반드시 `clone()` 을 적어야 해서**,
성능을 잡아먹는 복사가 눈에 보인다. (그래서 리뷰에서 "여기 clone 왜 있어요?"가 유효한 질문이 된다)

## 3. 빌림(borrow): `&T`, `&mut T`

매번 소유권을 넘길 수는 없으니 **잠깐 빌려주는** 방법이 있다.

| | 의미 | 동시에 가능한 개수 |
|---|---|---|
| `&T` | 읽기 전용 참조 | 여러 개 |
| `&mut T` | 수정 가능한 참조 | **하나만** (그동안 `&T` 도 불가) |

```rust
// crates/core/src/encode.rs — 읽기만 하므로 &
pub fn encode(image: &DynamicImage, format: OutputFormat, quality: u8) -> Result<Vec<u8>>

// crates/core/src/exif.rs — 제자리에서 2바이트를 고치므로 &mut
pub fn reset_orientation(raw_exif: &mut [u8])
```

이 규칙("읽는 사람 여럿 또는 쓰는 사람 하나")이 **데이터 레이스를 컴파일 타임에 막는다.**
JS 에서 흔한 "다른 곳에서 배열을 수정하는 바람에 순회가 깨지는" 버그도 같은 규칙으로 막힌다:

```rust
let mut v = vec![1, 2, 3];
for x in &v {        // v 를 빌리는 중
    v.push(*x);      // ❌ cannot borrow `v` as mutable because it is also borrowed as immutable
}
```

### 메서드의 self

```rust
fn as_str(self)            // 값을 가져감 (Copy 타입이라 부담 없음)
fn width(&self) -> u32     // 읽기만
fn take_bytes(&mut self)   // 내부를 바꿈
```

## 4. 슬라이스: `&[T]`, `&str`

**슬라이스는 "빌린 연속된 메모리 구간"** 이다. (포인터, 길이) 두 개의 값일 뿐이라 만드는 데 비용이 없다.

```rust
let input: &[u8] = ...;            // 소유하지 않은 바이트들
&webp[12..]                        // 12번째부터 끝까지 (복사 없음!)
let (header, body) = rest.split_at(8);   // 한 슬라이스를 둘로 (복사 없음)
```

`Vec<u8>` → `&[u8]` 변환은 자동이다(deref coercion). 그래서 함수 인자는 `&[u8]` 로 받는 게 관례:
`Vec` 도, 배열도, 다른 슬라이스의 일부도 전부 받을 수 있다.

```rust
decode(&vec)          // Vec<u8> → &[u8] 자동
decode(b"II*\0fake")  // &[u8; 8] → &[u8] 자동
decode(&buf[10..20])  // 부분 슬라이스
```

이 레포에서 제로카피가 일어나는 지점 정리:

```
JS Uint8Array ──복사①──▶ wasm 메모리 ──&[u8]──▶ Cursor ──▶ 디코더
                                          (빌림, 복사 없음)
디코더 ──새 할당──▶ DynamicImage ──이동──▶ resize ──새 할당──▶ 결과 (원본 해제)
결과 ──&빌림──▶ 인코더 ──새 할당──▶ Vec<u8> ──복사②──▶ JS Uint8Array
```

## 5. 라이프타임 `'a`

참조는 "가리키는 대상보다 오래 살면 안 된다". 보통은 컴파일러가 알아서 추론하지만,
**구조체가 참조를 품을 때**는 사람이 적어 줘야 한다.

```rust
// crates/core/src/webp_meta.rs
struct Chunk<'a> {
    fourcc: [u8; 4],
    payload: &'a [u8],      // "원본 버퍼('a)보다 오래 살 수 없다"
}

fn parse_chunks(webp: &[u8]) -> Result<Vec<Chunk<'_>>>   // '_ = "입력과 같은 수명"
```

이 덕분에 아래 코드는 **컴파일이 안 된다**:

```rust
let chunks = {
    let buf = read_file();          // buf 는 이 블록에서 죽는다
    parse_chunks(&buf)?             // ❌ `buf` does not live long enough
};
chunks[0].payload;                  // C 였다면 dangling pointer → 크래시 또는 보안 취약점
```

`'static` 은 특별한 라이프타임으로 "프로그램 내내 산다"는 뜻이다. 문자열 리터럴이 `&'static str` 인 이유.

```rust
pub fn as_str(self) -> &'static str { "webp" }
```

## 6. `Option<T>` 안의 값 빌리기

`Option<Vec<u8>>` 을 가진 채로 안쪽만 빌려 보고 싶을 때가 많다.

```rust
// crates/core/src/pipeline.rs
let summary = exif.as_deref().map(exif::summarize).unwrap_or_default();
//                 ^^^^^^^^^ Option<Vec<u8>> → Option<&[u8]>  (exif 의 소유권은 그대로)
...
let meta = Metadata { icc: icc.as_deref(), exif: kept_exif.as_deref() };
```

- `as_deref()`: `Option<Vec<u8>>` → `Option<&[u8]>`
- `as_ref()`: `Option<T>` → `Option<&T>`
- 이걸 안 쓰고 `exif.unwrap()` 하면 소유권을 가져와 버려서 나중에 다시 못 쓴다.

## 7. drop 순서와 `mem::take`

```rust
// crates/wasm/src/lib.rs
pub fn take_bytes(&mut self) -> Vec<u8> {
    std::mem::take(&mut self.bytes)   // 필드를 빈 Vec 으로 바꿔치기하고 원래 값을 가져온다
}
```

`&mut self` 로는 필드를 밖으로 "이동"시킬 수 없다(구조체에 구멍이 생기니까).
`mem::take` 는 그 자리에 기본값(`Vec::new()`, 할당 없음)을 채워 넣어서 규칙을 지키면서 값을 꺼낸다.
JS 에는 없는 관용구지만, "결과를 한 번만 꺼낼 수 있다"는 의미가 타입에 드러나서 오히려 명확하다.

## 8. 자주 만나는 에러와 해법

| 에러 | 뜻 | 보통의 해법 |
|---|---|---|
| `E0382: borrow of moved value` | 이동한 값을 또 씀 | 함수 인자를 `&T` 로 바꾸거나, 순서를 바꾸거나, `clone()` |
| `E0502: cannot borrow as mutable` | 읽기 빌림이 살아 있는데 수정 시도 | 빌림의 스코프를 좁히거나, 인덱스만 모아 두고 나중에 수정 |
| `E0499: cannot borrow twice as mutable` | `&mut` 두 개 | 한 번에 하나만, 또는 `split_at_mut` |
| `E0597: does not live long enough` | 참조가 대상보다 오래 삼 | 대상을 더 바깥 스코프로 올리거나, 소유하는 타입으로 반환 |
| `E0507: cannot move out of borrowed content` | 빌린 것에서 값을 꺼냄 | `clone()`, `mem::take`, 또는 `&` 로 처리 |

## 9. JS 에도 있는 "이동": Transferable

워커로 `ArrayBuffer` 를 transfer 하면 원래 쪽이 비워진다(detached). Rust 의 move 와 정확히 같은 개념이다.

```ts
// packages/pixelvault/src/worker.ts
return Comlink.transfer(result, [result.bytes.buffer]);
// 이 시점 이후 워커 쪽 result.bytes 는 길이 0
```

`docs/m3-worker.md` 에 이 이야기가 더 있다.

## 다음

→ [03. 타입 · 트레잇 · 제네릭](./03-traits-generics.md)
