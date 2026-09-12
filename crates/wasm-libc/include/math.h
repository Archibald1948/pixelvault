/* pixelvault-wasm-libc: wasm32-unknown-unknown 에는 C 표준 라이브러리가 없다.
 * libwebp(C)를 wasm 으로 컴파일하려면 헤더가 필요하므로, libwebp 가 실제로 쓰는
 * 함수만 선언한 최소 헤더를 제공한다. 구현은 ../src/lib.rs (Rust) 에 있다.
 *
 * wasm 이 아닌 타깃(네이티브 cargo test)에서는 #include_next 로 진짜 시스템 헤더를 쓴다. */
#ifndef __wasm__
#include_next <math.h>
#else
#ifndef PV_MATH_H
#define PV_MATH_H
/* wasm 명령어 하나로 끝나는 것들은 clang builtin 으로 */
#define sqrt(x) __builtin_sqrt(x)
#define sqrtf(x) __builtin_sqrtf(x)
#define floor(x) __builtin_floor(x)
#define ceil(x) __builtin_ceil(x)
#define fabs(x) __builtin_fabs(x)
/* 나머지는 Rust 의 compiler_builtins(libm 포팅)가 심볼을 제공한다 */
double pow(double x, double y);
double log(double x);
double log2(double x);
double log10(double x);
float logf(float x);
float expf(float x);
#endif
#endif
