/* pixelvault-wasm-libc: wasm32-unknown-unknown 에는 C 표준 라이브러리가 없다.
 * libwebp(C)를 wasm 으로 컴파일하려면 헤더가 필요하므로, libwebp 가 실제로 쓰는
 * 함수만 선언한 최소 헤더를 제공한다. 구현은 ../src/lib.rs (Rust) 에 있다.
 *
 * wasm 이 아닌 타깃(네이티브 cargo test)에서는 #include_next 로 진짜 시스템 헤더를 쓴다. */
#ifndef __wasm__
#include_next <assert.h>
#else
#ifndef PV_ASSERT_H
#define PV_ASSERT_H
/* libwebp 는 NDEBUG 로 빌드되므로 assert 는 항상 no-op 이다. */
#define assert(x) ((void)0)
#endif
#endif
