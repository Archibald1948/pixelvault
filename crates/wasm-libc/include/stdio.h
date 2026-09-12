/* pixelvault-wasm-libc: wasm32-unknown-unknown 에는 C 표준 라이브러리가 없다.
 * libwebp(C)를 wasm 으로 컴파일하려면 헤더가 필요하므로, libwebp 가 실제로 쓰는
 * 함수만 선언한 최소 헤더를 제공한다. 구현은 ../src/lib.rs (Rust) 에 있다.
 *
 * wasm 이 아닌 타깃(네이티브 cargo test)에서는 #include_next 로 진짜 시스템 헤더를 쓴다. */
#ifndef __wasm__
#include_next <stdio.h>
#else
#ifndef PV_STDIO_H
#define PV_STDIO_H
#include <stddef.h>
/* libwebp 의 stdio 사용은 전부 디버그 출력/CPU 정보 조회뿐이라 아무 일도 안 하게 만든다.
 * printf 류는 가변 인자 함수라 Rust stable 로는 정의할 수 없어서 매크로로 삼킨다. */
typedef struct PvFile FILE;
#define stderr ((FILE*)0)
#define stdout ((FILE*)0)
#define printf(...) 0
#define fprintf(...) 0
#define snprintf(...) 0
#define fopen(...) ((FILE*)0)
#define fclose(...) 0
#define fgets(...) ((char*)0)
#endif
#endif
