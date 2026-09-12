# Rust 깊이 읽기 — pixelvault 코드로 배우는 Rust

이 폴더는 **Rust 를 처음 보는 프론트엔드 개발자**가 이 레포의 코드를 끝까지 읽을 수 있게 하는 것이 목표다.
문법 기초부터 시작해서, 마지막에는 `crates/` 안의 모든 파일을 줄 단위로 해설한다.

> 짧게 훑고 싶으면 [../rust-concepts.md](../rust-concepts.md) (한 장짜리 요약 사전)부터 보자.
> 여기 문서들은 그 사전의 각 항목을 자세히 풀어 쓴 것이다.

## 읽는 순서

| # | 문서 | 내용 |
|---|---|---|
| 01 | [기초 문법](./01-basics.md) | 변수·타입·함수·제어문·구조체·enum·문자열·매크로 (TS 와 비교) |
| 02 | [소유권 · 빌림 · 라이프타임](./02-ownership.md) | Rust 의 핵심. 메모리를 GC 없이 관리하는 규칙 |
| 03 | [타입 · 트레잇 · 제네릭](./03-traits-generics.md) | impl, derive, 트레잇 바운드, 단형화, Cow, From/Into |
| 04 | [에러 처리](./04-error-handling.md) | Option, Result, `?`, thiserror, panic, wasm 경계까지 전달 |
| 05 | [컬렉션 · 이터레이터 · 클로저](./05-collections-iterators.md) | Vec, 슬라이스, 이터레이터 어댑터, 클로저 3종 |
| 06 | [모듈 · 크레이트 · Cargo](./06-modules-cargo.md) | 파일 구조, 가시성, feature, 프로필, 조건부 컴파일 |
| 07 | [unsafe 와 FFI](./07-unsafe-ffi.md) | raw 포인터, `extern "C"`, 메모리 할당기, C 와 주고받기 |
| 08 | [wasm-bindgen](./08-wasm-bindgen.md) | JS ↔ Rust 타입 변환이 실제로 어떻게 돌아가는가 |
| 09 | [테스트와 도구](./09-testing-tooling.md) | 테스트 작성법, clippy, fmt, 문서 주석, 크기 분석 |
| 10 | [코드 전체 해설](./10-code-walkthrough.md) | `crates/` 의 모든 파일을 순서대로 읽는다 |
| 11 | [성능과 메모리](./11-performance-memory.md) | 릴리스 프로필, 인라이닝, SIMD, wasm 메모리 모델 |

## 이 문서들의 원칙

1. **추상적인 예제 대신 이 레포의 진짜 코드**를 쓴다. 파일 경로가 적혀 있으면 열어서 같이 보자.
2. **TS/JS 와 비교**해서 "왜 이렇게 생겼는지"를 설명한다.
3. 컴파일러가 왜 화를 내는지, 그때 뭘 고쳐야 하는지를 같이 적는다.

## 빠른 참조: TS 개발자를 위한 대응표

| TypeScript | Rust | 비고 |
|---|---|---|
| `const x = 1` | `let x = 1;` | Rust 는 기본이 불변 |
| `let x = 1` | `let mut x = 1;` | `mut` 를 붙여야 바꿀 수 있다 |
| `number` | `u8 u32 u64 i32 f32 f64 usize` | 크기와 부호를 명시해야 한다 |
| `string` | `String` (소유) / `&str` (빌림) | |
| `T[]` | `Vec<T>` (소유) / `&[T]` (빌림) | |
| `T \| undefined` | `Option<T>` | `null` 이 없다 |
| `throw` / `try-catch` | `Result<T, E>` + `?` | 예외가 없다 |
| `interface` | `trait` | |
| `class` | `struct` + `impl` | 상속은 없다 |
| `type A = B \| C` | `enum` | 값을 품을 수 있다 |
| `as` | `as` (숫자 잘림) / `From`/`TryFrom` (안전) | |
| `import { x } from "./y"` | `use crate::y::x;` | |
| `package.json` | `Cargo.toml` | |
| `npm i` | `cargo build` (자동) | |
