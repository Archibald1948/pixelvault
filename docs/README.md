# pixelvault 학습 문서

스펙(`rust-pixelvault.md`)을 마일스톤 순서대로 구현하면서 내린 결정, 겪은 문제, Rust 학습 포인트를 정리했다.
**Rust 를 처음 본다면** [Rust 개념 정리](./rust-concepts.md) 를 먼저 훑고, 마일스톤 문서를 코드와 같이 읽는 걸 추천한다.

## 읽는 순서

1. [00 셋업](./00-setup.md) — 설치, 명령어, 폴더 구조
2. [M0 파이프라인 뚫기](./m0-pipeline.md) — Rust 함수를 브라우저에서 부르기
3. [M1 코어 파이프라인](./m1-core-pipeline.md) — decode → resize → encode, 소유권, wasm 크기 최적화
4. [libwebp on wasm](./libwebp-on-wasm.md) — C 라이브러리를 wasm 으로 (가장 까다로웠던 부분)
5. [M2 EXIF & BlurHash](./m2-exif-blurhash.md) — 사진이 눕는 버그, 메타데이터, 바이트 조립
6. [M3 Web Worker](./m3-worker.md) — 메인 스레드에서 빼기, 병렬 처리
7. [M4 벤치마크](./m4-benchmark.md) — Canvas API vs WASM, 측정 방법론
8. [M5 패키징 & 배포](./m5-packaging.md) — npm, Vercel, CI
9. [Rust 개념 정리](./rust-concepts.md) — 사전처럼

## 스펙의 "학습 포인트" 질문에 대한 답

| 질문 | 답이 있는 곳 |
|---|---|
| `&[u8]` 와 `Vec<u8>` 을 언제 나눠 쓰는가 (제로카피가 실제로 어디서 일어나는가) | [M1 ①](./m1-core-pipeline.md#학습-포인트-1-u8-vs-vecu8--제로카피는-어디서-일어나나), [개념 3](./rust-concepts.md#3-u8-vs-vecu8--슬라이스와-벡터) |
| `Result` + `thiserror` 로 에러를 WASM 경계까지 어떻게 전달하는가 (`JsValue` 변환) | [M1 ③](./m1-core-pipeline.md#학습-포인트-3-result---로-에러-전파), [개념 6](./rust-concepts.md#6-에러-thiserror-와-from) |
| `image::DynamicImage` 의 소유권이 파이프라인을 타고 어떻게 이동하는가 | [M1 ②](./m1-core-pipeline.md#학습-포인트-2-dynamicimage-의-소유권이-파이프라인을-타고-흐르는-모습), `crates/core/src/pipeline.rs` 상단 다이어그램 |
| 왜 `fast_image_resize` 가 `image::imageops::resize` 보다 빠른가 (SIMD, 커널 분리) | [M1 ④](./m1-core-pipeline.md#학습-포인트-4-왜-fast_image_resize-가-imageimageopsresize-보다-빠른가) |

## 스펙과 달라진 점 (그리고 이유)

| 스펙 | 실제 | 이유 |
|---|---|---|
| `opt-level = "z"` | `opt-level = 3` | 측정 결과 3 으로도 gzip 500KB 예산 안(411KB)이고 "z" 보다 1.6배 빠름 ([M1](./m1-core-pipeline.md#측정-wasm-크기-vs-속도-opt-level)) |
| (없음) | `crates/wasm-libc` 추가 | libwebp(C)를 wasm32 로 링크하려면 최소 libc 가 필요 ([libwebp-on-wasm](./libwebp-on-wasm.md)) |
| `image` 로 인코딩 | JPEG 는 `jpeg-encoder` | image 의 JPEG 인코더는 4:2:2 + 고정 허프만이라 24% 큼 ([M4](./m4-benchmark.md)) |
| (없음) | `packages/pixelvault` 추가 | npm 에 올릴 TS API + 워커 풀이 들어갈 자리 ([M3](./m3-worker.md)) |
| COOP/COEP 헤더 (함정 목록) | 사용 안 함 | 워커마다 독립 wasm 인스턴스라 SharedArrayBuffer 불필요 ([M3](./m3-worker.md#3-워커-풀--왜-워커마다-wasm-이-따로-있나)) |
| `fast_image_resize` 의 `image` feature | 쓰지 않음 | 모든 픽셀 타입 코드가 링크되어 wasm 이 +750KB ([M1 ⑤](./m1-core-pipeline.md#학습-포인트-5-제네릭과-wasm-크기--174-mb--098-mb)) |
| (없음) | ICC 프로파일 보존 | 아이폰(Display P3) 사진 색이 칙칙해지는 것 방지 ([M2](./m2-exif-blurhash.md#icc-색-프로파일은-항상-유지)) |
