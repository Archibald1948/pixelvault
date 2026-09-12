# C 라이브러리(libwebp)를 wasm 으로: `crates/wasm-libc` 가 존재하는 이유

> 이 프로젝트에서 가장 "Rust 답지 않은" 부분이지만, 실무에서 wasm 을 하다 보면 반드시 만나는 문제다.

## 문제 1: 헤더가 없다

`webp` 크레이트는 C 로 작성된 **libwebp** 를 감싼 바인딩이다. `libwebp-sys` 가 빌드 시점에 C 소스를 컴파일한다.
네이티브(macOS)에서는 문제없지만 `wasm32-unknown-unknown` 으로 빌드하면:

```
fatal error: 'inttypes.h' file not found
```

`wasm32-unknown-unknown` 의 `unknown` 은 "운영체제 없음"이라는 뜻이다. 운영체제가 없으니 **C 표준 라이브러리(libc)도 없다.**
`stdlib.h`, `string.h` 같은 헤더도, `malloc`/`free` 같은 함수 구현도 없다.

### 선택지

| 방법 | 장점 | 단점 |
|---|---|---|
| `image` 크레이트의 순수 Rust WebP 인코더 | 설정 0 | **무손실만** 지원 → 사진이 JPEG 보다 커짐. quality 옵션 무의미 |
| 다른 순수 Rust 손실 WebP 크레이트 | C 없음 | 쓸만한 건 AGPL 라이선스(`zenwebp`) → npm 패키지 사용자에게 전염. 나머지는 검증 안 된 0.x |
| wasi-sdk 의 libc 헤더 사용 | 정석 | 120 MB 다운로드, 모든 개발자·CI 가 설치 필요 |
| **필요한 만큼만 직접 만든 최소 libc** ✅ | 의존성 0, 레포 안에서 완결 | 헤더 6개 + Rust 함수 7개 유지보수 |

libwebp 소스에서 실제로 쓰는 libc 함수를 grep 해 보니 30개 남짓이었고, 그중 상당수는 디버그 출력용이었다. 그래서 마지막 방법을 택했다.

## 해결 1: 최소 헤더 (`crates/wasm-libc/include/*.h`)

```c
#ifndef __wasm__
#include_next <stdlib.h>      // 네이티브면 진짜 시스템 헤더로 넘긴다
#else
void* malloc(size_t size);    // wasm 이면 선언만 제공 (구현은 Rust 에)
...
static inline int abs(int x) { return x < 0 ? -x : x; }   // 간단한 건 헤더에서 끝냄
static inline char* getenv(const char* n) { return NULL; } // 브라우저엔 환경변수가 없다
#endif
```

- `.cargo/config.toml` 의 `[env] C_INCLUDE_PATH` 로 이 폴더를 C 컴파일러에 알려준다.
- `#include_next` 가드 덕분에 네이티브 `cargo test` 는 영향을 받지 않는다.
- `printf`/`fprintf` 는 **가변 인자 함수**라 stable Rust 로는 정의할 수 없다. libwebp 에서는 디버그 출력에만 쓰이므로 매크로로 삼켰다: `#define printf(...) 0`.
- `memcpy`, `memset`, `pow`, `log` 등은 Rust 의 `compiler_builtins` 가 wasm32 용으로 이미 제공하므로 선언만 있으면 된다.

## 문제 2: 링크가 "조용히" 실패한다

헤더 문제를 풀고 나니 빌드는 성공했다. 그런데 wasm 파일의 import 목록을 보면:

```js
WebAssembly.Module.imports(module)
// [{ module: 'env', name: 'WebPEncode' }, { module: 'env', name: 'WebPPictureFree' }, ...]
```

libwebp 함수들이 **wasm 안에 들어가지 않고 "JS 가 제공해 줘야 하는 import"로 남아 있었다.**
브라우저에서 instantiate 하는 순간 `LinkError` 로 터질 폭탄이다.

원인: C 오브젝트 파일들을 정적 라이브러리(`.a`)로 묶는 `ar` 도구. macOS 기본 `ar` 는 wasm 오브젝트를 이해하지 못해서
**심볼 인덱스를 만들지 못한다**(`ranlib: warning: archive member ... not a mach-o file`).
링커는 인덱스가 없으니 라이브러리 안에 `WebPEncode` 가 있는 줄 모르고, Rust 의 wasm 타깃은 기본적으로 모르는 심볼을 import 로 바꿔 버린다.

## 해결 2: llvm-ar + 빌드 후 검증

- `rustup component add llvm-tools` 로 LLVM 의 `llvm-ar` 를 받는다(wasm 오브젝트를 이해함).
- `scripts/build-wasm.sh` 가 `AR_wasm32_unknown_unknown` 환경변수로 `cc` 크레이트에게 이 `ar` 를 쓰라고 알려준다.
- 빌드 직후 wasm 의 import 를 검사해서 `env.*` 가 하나라도 있으면 **빌드를 실패**시킨다. "성공한 척하는 빌드"를 막는 안전장치.

> 링커 옵션(`--unresolved-symbols=report-all`)으로 막으려 했지만 실패했다. rustc 가 `extern "C"` 함수를 "명시적으로 env 에서 import 하는 심볼"로 표시하기 때문에 링커가 에러로 보지 않는다. 그래서 결과물을 직접 검사하는 방식을 택했다.

## 문제 3: `malloc`/`free`/`qsort` 구현

인덱스가 생기자 남은 미해결 심볼은 5개였다: `malloc`, `free`, `calloc`, `qsort`, `bsearch`.
`crates/wasm-libc/src/lib.rs` 에서 Rust 로 구현했다.

### malloc/free: 크기를 어디에 기억할까

C 의 `free(ptr)` 는 크기를 받지 않는다. 그런데 Rust 의 `dealloc(ptr, layout)` 은 **할당할 때의 크기와 정렬을 다시 달라고** 한다.
그래서 16바이트를 더 할당해서 앞부분에 크기를 적어 두는 고전적인 방법을 쓴다:

```
[ 전체 크기(usize) + 패딩 ][ 사용자에게 주는 영역 ................ ]
^ Rust alloc 이 준 주소      ^ malloc 이 반환하는 주소 (= base + 16)
```

`free(ptr)` 는 `ptr - 16` 에서 크기를 읽어 `Layout` 을 복원한 뒤 `dealloc` 한다.

### `#[unsafe(no_mangle)] pub unsafe extern "C" fn`

```rust
#[unsafe(no_mangle)]
pub unsafe extern "C" fn malloc(size: usize) -> *mut c_void { ... }
```

- **`extern "C"`**: C 의 호출 규약(인자를 어떻게 넘기는지)을 따르라는 뜻. C 코드가 부를 수 있게 된다.
- **`no_mangle`**: Rust 는 보통 함수 이름을 `_ZN4core3ptr...` 처럼 변형(mangling)해서 충돌을 피한다. C 는 정확히 `malloc` 이라는 이름을 찾으므로 변형을 끈다. 전역 이름을 차지하는 위험한 동작이라 2024 에디션부터 `unsafe(...)` 로 감싸야 한다.
- **`unsafe fn`**: 호출자가 지켜야 할 규칙(예: `free` 에는 `malloc` 이 준 포인터만 넘길 것)이 있고 컴파일러가 검사할 수 없다는 표시. 문서의 `# Safety` 절에 그 규칙을 적는 것이 관례다.
- **`*mut c_void`**: C 의 `void*`. Rust 의 참조(`&`)와 달리 **raw 포인터**는 빌림 검사를 받지 않는다. 역참조는 `unsafe` 블록 안에서만 가능하다.

## 정리: 이 문제에서 배운 것

1. `wasm32-unknown-unknown` 은 진짜로 **아무것도 없는** 환경이다. Rust std 는 동작하도록 만들어져 있지만, C 코드는 그렇지 않다.
2. 빌드 성공 ≠ 링크 성공. wasm 은 모르는 심볼을 import 로 미뤄 버리므로 **결과물의 import 목록**을 확인하는 습관이 중요하다.
3. FFI(다른 언어와의 경계)는 `unsafe` 가 모이는 곳이다. 그래서 별도 크레이트로 격리하고, 각 함수에 안전 조건을 문서화했다.
4. 이 모든 게 wasm 에서만 필요하므로 `#![cfg(target_arch = "wasm32")]` 로 크레이트 전체를 조건부 컴파일한다. 네이티브에서는 빈 크레이트다.
