/* pixelvault-wasm-libc: wasm32-unknown-unknown 에는 C 표준 라이브러리가 없다.
 * libwebp(C)를 wasm 으로 컴파일하려면 헤더가 필요하므로, libwebp 가 실제로 쓰는
 * 함수만 선언한 최소 헤더를 제공한다. 구현은 ../src/lib.rs (Rust) 에 있다.
 *
 * wasm 이 아닌 타깃(네이티브 cargo test)에서는 #include_next 로 진짜 시스템 헤더를 쓴다. */
#ifndef __wasm__
#include_next <string.h>
#else
#ifndef PV_STRING_H
#define PV_STRING_H
#include <stddef.h>
/* 이 함수들은 Rust 의 compiler_builtins 가 wasm32 용으로 이미 제공한다. 선언만 하면 된다. */
void* memcpy(void* dst, const void* src, size_t n);
void* memmove(void* dst, const void* src, size_t n);
void* memset(void* dst, int c, size_t n);
int memcmp(const void* a, const void* b, size_t n);
size_t strlen(const char* s);
int strcmp(const char* a, const char* b);
int strncmp(const char* a, const char* b, size_t n);
#endif
#endif
