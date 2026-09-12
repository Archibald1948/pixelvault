/* pixelvault-wasm-libc: wasm32-unknown-unknown 에는 C 표준 라이브러리가 없다.
 * libwebp(C)를 wasm 으로 컴파일하려면 헤더가 필요하므로, libwebp 가 실제로 쓰는
 * 함수만 선언한 최소 헤더를 제공한다. 구현은 ../src/lib.rs (Rust) 에 있다.
 *
 * wasm 이 아닌 타깃(네이티브 cargo test)에서는 #include_next 로 진짜 시스템 헤더를 쓴다. */
#ifndef __wasm__
#include_next <stdlib.h>
#else
#ifndef PV_STDLIB_H
#define PV_STDLIB_H
#include <stddef.h>

/* Rust 쪽(src/lib.rs)에서 #[no_mangle] extern "C" 로 구현 */
void* malloc(size_t size);
void* calloc(size_t n, size_t size);
void* realloc(void* p, size_t size);
void free(void* p);
void qsort(void* base, size_t n, size_t size, int (*cmp)(const void*, const void*));
void* bsearch(const void* key, const void* base, size_t n, size_t size,
              int (*cmp)(const void*, const void*));
void abort(void) __attribute__((noreturn));

/* 헤더에서 바로 끝나는 것들 */
static inline int abs(int x) { return x < 0 ? -x : x; }
/* 디버그용 환경변수(MALLOC_LIMIT 등) 조회 — 브라우저엔 환경변수가 없다 */
static inline char* getenv(const char* name) { (void)name; return NULL; }
static inline int atoi(const char* s) { (void)s; return 0; }
#endif
#endif
