# 07. `unsafe` 와 FFI — C 와 주고받기

이 프로젝트의 `unsafe` 는 전부 `crates/wasm-libc/src/lib.rs` 한 파일에 모여 있다.
왜 필요했고, 무엇을 조심해야 하는지 정리한다. (배경 이야기: [../libwebp-on-wasm.md](../libwebp-on-wasm.md))

## 1. `unsafe` 가 푸는 다섯 가지 잠금

`unsafe` 는 "검사를 끄는 스위치"가 아니라, **컴파일러가 검증할 수 없는 5가지 행동을 허용**하는 표시다.

1. raw 포인터 역참조
2. `unsafe` 함수 호출 (FFI 포함)
3. 가변 static 접근/수정
4. `unsafe` 트레잇 구현
5. union 필드 접근

`unsafe` 블록 안에서도 빌림 검사, 타입 검사는 그대로 작동한다.
바뀌는 건 **"이 조건은 네가 지켜야 해"** 라는 책임이 사람에게 넘어온다는 점이다.

## 2. raw 포인터

```rust
*const T    // 읽기용
*mut T      // 쓰기용
```

참조(`&T`)와 달리 라이프타임이 없고, null 일 수 있고, 정렬/유효성이 보장되지 않는다.
만드는 것 자체는 안전하고, **역참조할 때만** `unsafe` 가 필요하다.

```rust
// crates/wasm-libc/src/lib.rs
unsafe fn block_of(ptr: *mut c_void) -> (*mut u8, Layout) {
    unsafe {
        let base = (ptr as *mut u8).sub(HEADER);       // 포인터 산술
        let total = (base as *const usize).read();     // 역참조해서 읽기
        (base, Layout::from_size_align_unchecked(total, HEADER))
    }
}
```

자주 쓰는 포인터 메서드: `.add(n)`, `.sub(n)`, `.read()`, `.write(v)`, `.cast::<U>()`, `.is_null()`.

슬라이스로 바꾸려면 길이를 사람이 보증해야 한다:

```rust
let bytes = core::slice::from_raw_parts_mut(base as *mut u8, n * size);
// "base 부터 n*size 바이트가 유효하고, 그동안 다른 곳에서 건드리지 않는다"를 내가 약속한 것
```

## 3. `extern "C"` — C 호출 규약

```rust
#[unsafe(no_mangle)]
pub unsafe extern "C" fn malloc(size: usize) -> *mut c_void { ... }
```

| 요소 | 의미 |
|---|---|
| `extern "C"` | 인자 전달 방식(ABI)을 C 방식으로. C 코드가 부를 수 있게 된다 |
| `no_mangle` | Rust 는 보통 심볼 이름을 변형(mangle)해서 충돌을 막는데, C 는 정확히 `malloc` 을 찾으므로 끈다 |
| `unsafe(...)` | 2024 에디션에서 추가. "전역 심볼 이름을 차지하는 위험한 속성"임을 표시 |
| `unsafe fn` | 호출자가 지켜야 할 조건이 있음. `# Safety` 문서로 그 조건을 적는 게 관례 |

반대 방향(Rust 가 C 함수를 부름)은 `extern` 블록이다. `libwebp-sys` 가 이렇게 생겼다:

```rust
unsafe extern "C" {
    pub fn WebPEncode(config: *const WebPConfig, picture: *mut WebPPicture) -> c_int;
}
```

우리는 그 위의 안전한 래퍼(`webp` 크레이트)를 쓰므로 직접 부를 일은 없다.

## 4. C 타입 대응

| C | Rust |
|---|---|
| `void*` | `*mut c_void` |
| `const void*` | `*const c_void` |
| `size_t` | `usize` |
| `int` | `c_int` (= `i32`) |
| `char*` | `*mut c_char` (문자열은 `CStr`/`CString` 으로 다룬다) |
| 함수 포인터 | `unsafe extern "C" fn(...) -> ...` |

```rust
type CompareFn = unsafe extern "C" fn(*const c_void, *const c_void) -> c_int;
```

C 의 `qsort` 비교 함수처럼 **콜백을 받는** 경우에 쓴다.

## 5. 직접 만든 메모리 할당기

C 의 `free(ptr)` 는 크기를 받지 않는데 Rust 의 `dealloc(ptr, layout)` 은 요구한다. 그래서 헤더를 붙인다:

```
[ total_size: usize | 패딩 ][ 사용자에게 주는 영역 ................ ]
^ Rust alloc 이 준 주소      ^ malloc 이 반환 (= base + 16)
```

```rust
const HEADER: usize = 16;   // usize 를 담을 공간 + C 가 기대하는 최대 정렬(max_align_t)

fn layout_for(user_size: usize) -> Option<Layout> {
    let total = user_size.checked_add(HEADER)?;        // 오버플로 방지
    Layout::from_size_align(total, HEADER).ok()
}

pub unsafe extern "C" fn calloc(n: usize, size: usize) -> *mut c_void {
    let Some(layout) = n.checked_mul(size).and_then(layout_for) else {
        return core::ptr::null_mut();                  // C 표준: 오버플로면 NULL
    };
    ...alloc_zeroed(layout)...
}
```

**여기서 배울 점**

- 할당 실패는 panic 이 아니라 **NULL 반환**이어야 한다(C 쪽이 NULL 검사를 한다).
- `checked_mul`/`checked_add` 로 정수 오버플로를 막지 않으면 "작은 버퍼를 할당하고 큰 범위를 쓰는" 전형적인 힙 오버플로 취약점이 된다.
- `Layout::from_size_align_unchecked` 는 검사를 생략하는 버전이다. 우리가 방금 그 값으로 할당했음을 아는 자리에서만 쓴다.

## 6. C 콜백을 안전하게 감싸기 — `qsort`

원소 크기가 런타임 값이라 Rust 의 제네릭 `sort` 를 쓸 수 없다. 인덱스를 정렬한 뒤 바이트를 재배치했다:

```rust
let bytes = core::slice::from_raw_parts_mut(base as *mut u8, n * size);
let original = bytes.to_vec();                       // 원본 스냅샷
let elem = |i: usize| original.as_ptr().add(i * size).cast::<c_void>();

let mut order: Vec<usize> = (0..n).collect();
order.sort_unstable_by(|&a, &b| cmp(elem(a), elem(b)).cmp(&0));   // C 비교 함수 호출

for (dst, &src) in order.iter().enumerate() {
    bytes[dst * size..(dst + 1) * size].copy_from_slice(&original[src * size..(src + 1) * size]);
}
```

스냅샷을 뜨는 이유: 정렬 중에 원본을 옮기면서 비교하면 C 비교 함수가 **이동 중인 메모리**를 볼 수 있다.
(성능보다 정확성을 택했다. libwebp 가 qsort 를 쓰는 곳은 팔레트/허프만 트리라 배열이 작다)

## 7. `# Safety` 문서를 쓰는 이유

clippy 는 `pub unsafe fn` 에 `# Safety` 절이 없으면 경고한다(`missing_safety_doc`). 형식적인 규칙이 아니라,
**호출자가 지켜야 할 계약을 적는 자리**다.

```rust
/// C 의 `free`.
///
/// # Safety
/// `ptr` 은 null 이거나 이 모듈이 할당한, 아직 해제되지 않은 포인터여야 한다. (이중 해제 금지)
```

## 8. unsafe 를 다루는 실전 규칙

1. **격리한다**: `unsafe` 를 쓰는 코드를 한 모듈/크레이트에 모으고, 밖으로는 안전한 API 만 노출한다.
   이 레포는 `crates/wasm-libc` 하나에 전부 모여 있어서, 나머지 코드는 전부 safe Rust 다.
2. **최소화한다**: `unsafe` 블록은 꼭 필요한 줄만 감싼다.
3. **문서화한다**: 왜 안전한지(불변식)를 주석으로 남긴다.
4. **테스트한다**: 이 레포는 wasm 런타임 테스트에서 알파 WebP 인코딩을 돌려 `qsort`/`calloc` 경로를 실제로 실행한다.
5. **가능하면 피한다**: 순수 Rust 대안이 있으면 그쪽이 낫다. 여기서는 "손실 WebP 인코딩"이라는 요구 때문에 C 를 택했다.

## 9. C 라이브러리를 wasm 으로 올릴 때의 체크리스트

`docs/libwebp-on-wasm.md` 의 요약:

- [ ] C 헤더가 있는가? (`wasm32-unknown-unknown` 에는 libc 헤더가 없다)
- [ ] `malloc`/`free` 등 런타임 함수가 제공되는가?
- [ ] 아카이버가 wasm 오브젝트를 이해하는가? (macOS 기본 `ar` 는 못 한다 → `llvm-ar`)
- [ ] 빌드 결과의 **import 목록에 `env.*` 가 남아 있지 않은가?** (남아 있으면 링크 실패를 못 잡은 것)

마지막 항목은 `scripts/build-wasm.sh` 가 자동으로 검사한다.

## 다음

→ [08. wasm-bindgen](./08-wasm-bindgen.md)
