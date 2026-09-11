# pixelvault

> 브라우저 안에서 끝나는 이미지 처리 파이프라인 — Rust → WebAssembly + Next.js

**레포명**: `pixelvault` (대안: `clientside-image-lab`, `wasm-imagekit`)
**언어**: Rust (edition 2024) / TypeScript
**예상 기간**: 2~3주 (M3까지 기준)

---

## 1. 무엇을 만드는가

이미지를 **서버에 올리지 않고** 브라우저 메모리 안에서 리사이즈 · 포맷 변환 · EXIF 제거 · BlurHash 생성까지 끝내는 WASM 엔진과, 그것을 쓰는 Next.js 데모 앱.

최종 산출물은 두 개입니다.

1. npm 패키지 (`@<사용자>/pixelvault`) — 남이 설치해서 쓸 수 있는 것
2. 벤치마크가 포함된 데모 사이트 (Vercel 배포)

## 2. 왜 의미 있는가

- 이미지 업로드는 실제 서비스에서 **서버 비용 · 대역폭 · 프라이버시** 세 가지 문제의 근원입니다. 클라이언트에서 처리하면 셋 다 동시에 해결됩니다.
- 프론트엔드 배경을 그대로 활용하면서 Rust의 핵심 개념(소유권, borrow, 제로카피 슬라이스, `Result` 에러 전파)을 만나게 됩니다.
- "Rust 배웠어요"가 아니라 "이 npm 패키지 제가 만들었습니다"가 되는 프로젝트입니다.

## 3. 기술 스택

**Rust 쪽**

| 목적 | 크레이트 |
|------|---------|
| WASM 바인딩 | `wasm-bindgen`, `wasm-pack` |
| 디코드/인코드 | `image` (필요한 포맷만 feature 선택) |
| 고속 리사이즈 | `fast_image_resize` (SIMD) |
| WebP 인코딩 | `webp` |
| EXIF 파싱/제거 | `kamadak-exif` |
| BlurHash | `blurhash` |
| 에러 | `thiserror` |

**프론트 쪽**: Next.js (App Router), TypeScript, Web Worker, `comlink`

> AVIF(`ravif`)는 인코딩이 느리고 WASM 빌드가 까다롭습니다. **M1에서 제외**하고 여유 생기면 붙이세요.

## 4. 아키텍처 — 이 분리가 프로젝트의 성패를 가릅니다

```
pixelvault/
├── crates/
│   ├── core/              # 순수 Rust. wasm 의존성 0. 여기서 cargo test.
│   │   └── src/
│   │       ├── decode.rs
│   │       ├── resize.rs
│   │       ├── encode.rs
│   │       ├── exif.rs
│   │       ├── blurhash.rs
│   │       └── pipeline.rs   # 위를 조립하는 오케스트레이터
│   └── wasm/              # wasm-bindgen 바인딩만. 로직 없음.
├── pkg/                   # wasm-pack 출력 (gitignore)
├── www/                   # Next.js 데모 + 벤치마크 페이지
└── bench/
```

**원칙**: 모든 로직은 `core`에 두고 네이티브에서 `cargo test`로 검증합니다. `wasm` 크레이트는 타입 변환만 하는 얇은 껍데기입니다.
WASM 안에서의 디버깅은 로그가 거의 안 나와서 지옥입니다. 이 분리가 없으면 M2쯤에서 막힙니다.

## 5. 공개 API 설계

```ts
export interface ProcessOptions {
  maxWidth?: number;
  maxHeight?: number;
  format: 'webp' | 'jpeg' | 'png';
  quality?: number;        // 1-100, 기본 82
  stripExif?: boolean;     // 기본 true
  blurhash?: boolean;      // 기본 false
}

export interface ProcessResult {
  bytes: Uint8Array;
  width: number;
  height: number;
  format: string;
  originalBytes: number;
  outputBytes: number;
  blurhash?: string;
  elapsedMs: number;
}

export function processImage(
  input: Uint8Array,
  options: ProcessOptions
): Promise<ProcessResult>;
```

## 6. 마일스톤

### M0 — 파이프라인 뚫기
`wasm-pack` 셋업 후 `add(a, b)` 수준의 함수를 Next.js에서 호출해 화면에 찍기.
빌드 → 번들 → 로드 경로가 한 번 뚫리면 나머지는 내용물 채우는 일입니다. 여기에 하루 쓰는 게 정상입니다.

### M1 — 코어 파이프라인
`decode → resize → encode` (webp/jpeg/png).
`core` 크레이트에 함수별 유닛테스트 작성. 테스트용 샘플 이미지를 `crates/core/tests/fixtures/`에 넣어두세요.

### M2 — EXIF & BlurHash
- EXIF 스트립 (기본 동작)
- **Orientation 태그 보정** — EXIF를 지우기 전에 회전을 픽셀에 반영해야 합니다. 안 하면 아이폰 사진이 눕습니다. 실무에서 제일 흔한 버그.
- BlurHash 문자열 생성 (플레이스홀더용)

### M3 — Web Worker + 병렬 처리
메인 스레드에서 빼서 Worker로. `comlink`로 감싸고 여러 파일 순차/병렬 처리 + 진행률 콜백.

### M4 — 벤치마크 페이지
같은 작업을 **Canvas API(순수 JS) vs WASM**으로 돌려서 처리 시간 / 출력 용량 / 품질을 표로 비교.
이 페이지가 이 프로젝트의 하이라이트입니다. 숫자가 없으면 "만들어봤다"에서 끝납니다.

### M5 — 패키징 & 배포
npm 배포, README에 벤치마크 표 + GIF 데모, Vercel 배포.

## 7. 완료 기준 (Definition of Done)

- [ ] 10MB JPEG → 1920px WebP 변환 중 메인 스레드 블로킹이 없다 (DevTools Performance로 확인)
- [ ] 출력물에 EXIF GPS 태그가 남아있지 않다 (`cargo test`로 검증)
- [ ] 아이폰 세로 사진(Orientation=6)이 눕지 않는다
- [ ] Canvas API 대비 비교표가 README에 있다
- [ ] `.wasm` 파일이 gzip 기준 500KB 이하
- [ ] `cargo test` 전부 통과, `cargo clippy -- -D warnings` 통과

## 8. 함정 목록

| 함정 | 대응 |
|------|------|
| WASM 바이너리가 몇 MB로 부풀음 | `image` 크레이트를 `default-features = false`로 두고 필요한 포맷만. `[profile.release] opt-level="z", lto=true, codegen-units=1`. 빌드 후 `wasm-opt -Oz` |
| 큰 이미지에서 메모리 폭발 | wasm32는 주소 공간 4GB 한계. 디코드된 RGBA는 `가로×세로×4` 바이트임을 계산해서 상한 검증 |
| `SharedArrayBuffer` 쓰려니 안 됨 | COOP/COEP 헤더 필요. `next.config.js`의 `headers()`에서 `Cross-Origin-Opener-Policy: same-origin`, `Cross-Origin-Embedder-Policy: require-corp` |
| JS↔WASM 경계에서 큰 배열 복사 비용 | `Uint8Array`를 넘길 때 복사가 일어남. 큰 파일은 한 번만 넘기고 결과도 한 번만 받도록 API 설계 |
| Next.js가 .wasm을 못 불러옴 | `wasm-pack build --target web`으로 빌드하고 동적 import + `init()` 호출. Turbopack/webpack 설정 차이 주의 |

## 9. Claude Code 첫 지시 예시

```
이 스펙(01-rust-pixelvault.md)의 M0과 M1만 구현해줘.

- crates/core는 순수 Rust로만. wasm 의존성 넣지 마.
- decode/resize/encode 각 함수에 유닛테스트를 붙이고 fixtures에 테스트 이미지 추가.
- crates/wasm은 wasm-bindgen 바인딩만 얇게.
- www/는 Next.js App Router. 파일 하나 업로드 → 결과 미리보기 + 원본/결과 용량 비교만.

M2 이후는 아직 하지 마. 각 단계에서 왜 그 크레이트/자료구조를 골랐는지 짧게 설명해줘.
```

## 10. 학습 포인트 (여기서 물어보세요)

- `&[u8]`와 `Vec<u8>`을 언제 나눠 쓰는가 (제로카피가 실제로 어디서 일어나는가)
- `Result` + `thiserror`로 에러를 WASM 경계까지 어떻게 전달하는가 (`JsValue` 변환)
- `image::DynamicImage`의 소유권이 파이프라인을 타고 어떻게 이동하는가
- 왜 `fast_image_resize`가 `image::imageops::resize`보다 빠른가 (SIMD, 커널 분리)

---

## 부록 — 크레이트 최신 안정 버전 (crates.io 확인, 2026-09-08)

```toml
[dependencies]
image = { version = "0.25", default-features = false, features = ["jpeg", "png", "webp"] }
fast_image_resize = "6.1"
kamadak-exif = "0.6"
blurhash = "0.2"
webp = "0.3"
thiserror = "2.0"
wasm-bindgen = "0.2"
```
