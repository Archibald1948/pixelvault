# 08. wasm-bindgen — JS ↔ Rust 경계가 실제로 하는 일

`#[wasm_bindgen]` 한 줄이 무엇을 만들어 내는지, 생성된 코드를 직접 열어서 확인한다.

## 1. 문제: wasm 은 숫자만 안다

WebAssembly 함수가 주고받을 수 있는 값은 `i32`, `i64`, `f32`, `f64` 뿐이다(그리고 참조 타입).
문자열도, 배열도, 객체도 없다. 그런데 우리는 이런 걸 부르고 싶다:

```ts
const result = processImageRaw(uint8Array, 1920, undefined, "webp", 82, true, false);
result.blurhash;   // string | undefined
result.takeBytes(); // Uint8Array
```

이 간극을 메우는 게 wasm-bindgen 이다. **Rust 매크로 + JS glue 코드 생성기**의 조합으로,
"포인터와 길이를 주고받는 코드"를 자동으로 써 준다.

## 2. 빌드 파이프라인

```
crates/wasm/src/lib.rs
   │  cargo build --target wasm32-unknown-unknown   (매크로가 메타데이터를 .wasm 안에 심는다)
   ▼
pixelvault_wasm.wasm (원시)
   │  wasm-bindgen CLI   (메타데이터를 읽어 JS glue + .d.ts 생성, wasm 에서 메타데이터 제거)
   ▼
pkg/pixelvault.js + pkg/pixelvault.d.ts + pkg/pixelvault_bg.wasm
   │  wasm-opt (바이너리 최적화)
   ▼
최종 산출물
```

`wasm-pack build --target web` 이 이 과정을 한 번에 돌린다(`scripts/build-wasm.sh`).

## 3. 값이 실제로 어떻게 건너가는가

생성된 `pkg/pixelvault.js` 를 보면 전부 드러난다:

```js
export function processImageRaw(input, max_width, max_height, format, quality, strip_exif, blurhash) {
    const ptr0 = passArray8ToWasm0(input, wasm.__wbindgen_malloc);   // ① 바이트 복사
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passStringToWasm0(format, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);  // ② 문자열 UTF-8 인코딩 후 복사
    const len1 = WASM_VECTOR_LEN;
    const ret = wasm.processImageRaw(
        ptr0, len0,
        isLikeNone(max_width) ? Number.MAX_SAFE_INTEGER : (max_width) >>> 0,   // ③ Option 은 센티널 값으로
        isLikeNone(max_height) ? Number.MAX_SAFE_INTEGER : (max_height) >>> 0,
        ptr1, len1, quality, strip_exif, blurhash);
    if (ret[2]) { throw takeFromExternrefTable0(ret[1]); }                      // ④ Err 면 throw
    return RawProcessResult.__wrap(ret[0]);                                     // ⑤ 포인터를 JS 클래스로 감쌈
}
```

① **`&[u8]`**: JS 의 `Uint8Array` 를 wasm 메모리에 `malloc` 하고 복사한다. Rust 쪽은 그 영역을 슬라이스로 빌려 본다.
  → 큰 파일은 여기서 딱 한 번 복사된다. API 를 "한 번에 다 넘기고 한 번에 다 받는" 모양으로 설계한 이유.

② **`&str`**: UTF-8 로 인코딩해서 복사.

③ **`Option<u32>`**: `undefined` 를 `Number.MAX_SAFE_INTEGER` 라는 **센티널 값**으로 바꿔 보낸다(별도 플래그 인자 없이).
  주의: `max_width: -1` 을 넘기면 `>>> 0` 때문에 `4294967295` 가 된다. **범위 검사는 JS 쪽에서 해야 한다** →
  `packages/pixelvault/src/core.ts` 의 `validate()`.

④ **`Result`**: 성공/실패 플래그와 값을 함께 돌려받아, 실패면 JS 예외로 던진다.

⑤ **구조체 반환**: Rust 구조체는 wasm 메모리에 남고 JS 는 **포인터만** 받는다.

### 숫자 타입의 함정

`quality` 는 그냥 숫자로 전달된다. wasm 함수 시그니처는 `i32` 이고 Rust 는 `u8` 로 받으므로,
**300 을 넘기면 44 가 된다**(하위 8비트). 조용히 잘못된 결과가 나오는 걸 막으려고 TS 래퍼에서 먼저 검사한다:

```ts
if (o.quality !== undefined && !(Number.isInteger(o.quality) && o.quality >= 1 && o.quality <= 100)) {
  throw new RangeError(`quality must be an integer 1-100, got ${o.quality}`);
}
```

## 4. 구조체를 JS 클래스로 노출하기

```rust
#[wasm_bindgen]
pub struct RawProcessResult { bytes: Vec<u8>, width: u32, ... }   // 필드는 비공개

#[wasm_bindgen]
impl RawProcessResult {
    #[wasm_bindgen(getter)]                      // JS: result.width
    pub fn width(&self) -> u32 { self.width }

    #[wasm_bindgen(getter, js_name = mimeType)]  // Rust 는 snake_case, JS 는 camelCase
    pub fn mime_type(&self) -> String { self.format.mime_type().to_owned() }

    #[wasm_bindgen(js_name = takeBytes)]
    pub fn take_bytes(&mut self) -> Vec<u8> { std::mem::take(&mut self.bytes) }
}
```

생성된 JS:

```js
takeBytes() {
    const ret = wasm.rawprocessresult_takeBytes(this.__wbg_ptr);
    var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();   // wasm 메모리 → JS 로 복사
    wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);            // wasm 쪽 버퍼 해제
    return v1;
}
```

`mem::take` 로 소유권을 꺼내 돌려주면, glue 가 JS 배열로 복사하고 wasm 쪽 메모리를 바로 해제한다.
만약 getter 로 `Vec<u8>` 을 돌려줬다면 매번 `clone()` 이 필요했을 것이다(결과가 수 MB 라면 큰 차이).

## 5. 메모리 관리: `.free()`

```js
const RawProcessResultFinalization = new FinalizationRegistry(ptr => wasm.__wbg_rawprocessresult_free(ptr, 1));
```

- wasm-bindgen 은 `FinalizationRegistry` 를 등록해 두므로, JS GC 가 객체를 수거하면 **결국** wasm 메모리도 해제된다.
- 하지만 GC 시점은 보장되지 않는다. 이미지 수십 장을 연속 처리하면 그 사이 메모리가 계속 쌓인다.
  그래서 다 쓴 직후 **명시적으로 `.free()`** 를 부른다:

```ts
// packages/pixelvault/src/core.ts
try {
  return { bytes: raw.takeBytes(), width: raw.width, ... };
} finally {
  raw.free();
}
```

`free()` 후에 그 객체를 다시 쓰면 JS 쪽에서 "null pointer passed to rust" 에러가 난다.

## 6. 타입 대응표

| Rust | JS | 비용 |
|---|---|---|
| `u8 u32 i32 f64` | `number` | 없음 (범위 검사 없음!) |
| `bool` | `boolean` | 없음 |
| `&[u8]` | `Uint8Array` | JS → wasm 복사 |
| `Vec<u8>` (반환) | `Uint8Array` | wasm → JS 복사 + wasm 쪽 해제 |
| `&str` / `String` | `string` | UTF-8 변환 + 복사 |
| `Option<T>` | `T \| undefined` | 센티널 또는 플래그 |
| `Result<T, JsError>` | 성공값 또는 `throw` | |
| `#[wasm_bindgen] struct` | 클래스(포인터 래퍼) | 포인터만, `.free()` 필요 |
| `js_sys::*`, `web_sys::*` | 실제 JS 객체 | externref 테이블 경유 |

## 7. `--target` 별 로드 방식

```js
// --target web (이 프로젝트)
import init, { processImageRaw } from "./pixelvault.js";
await init();                    // fetch + WebAssembly.instantiate
// 또는 Node 에서:
initSync({ module: fs.readFileSync("pixelvault_bg.wasm") });
```

- `web`: `.wasm` 을 `new URL('pixelvault_bg.wasm', import.meta.url)` 로 가리킨다 → 번들러가 에셋으로 처리.
- `bundler`: `import` 만 하면 되지만 번들러가 wasm 을 지원해야 한다.
- `nodejs`: `require` 로 동기 로드.

이 프로젝트가 `web` 을 고른 이유는 `docs/m0-pipeline.md` 참고.

## 8. wasm 환경의 제약 (Rust 코드가 알아야 할 것)

| 제약 | 영향 | 이 레포의 대응 |
|---|---|---|
| 주소 공간 32비트 (최대 4 GiB) | 큰 이미지에서 메모리 부족 | 디코드 전에 `가로×세로×4` 상한 검사 |
| 시계가 없다 (`Instant::now()` panic) | 시간 측정 불가 | JS 에서 `performance.now()` 로 측정 |
| 파일 시스템·환경변수 없음 | `std::fs`, `getenv` 사용 불가 | libc 셈의 `getenv` 는 항상 NULL |
| 스레드 없음 (기본 설정) | `std::thread` 사용 불가 | 워커 여러 개로 병렬화 |
| panic = 트랩 | 스택 트레이스가 없다 | 로직은 네이티브에서 테스트 |
| `println!` 이 아무 데도 안 나옴 | 디버깅 어려움 | **core 를 네이티브로 테스트**하는 구조 |

마지막 두 줄이 이 프로젝트 구조(`core` 는 순수 Rust, `wasm` 은 얇은 껍데기)의 이유다.

## 9. 더 알아볼 만한 것

- `wasm-bindgen-test`: wasm 런타임에서 테스트 실행 (09 문서)
- `js-sys` / `web-sys`: JS 표준 객체와 DOM API 를 Rust 에서 부르기 (이 레포는 쓰지 않았다 — 경계를 얇게 유지)
- `serde-wasm-bindgen`: 구조체를 JS 객체로 직렬화. 편하지만 코드 크기가 늘어서, 여기서는 평평한 인자를 택했다.

## 다음

→ [09. 테스트와 도구](./09-testing-tooling.md)
