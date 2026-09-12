# M0 — 파이프라인 뚫기: Rust 함수를 브라우저에서 부르기

> 목표: `add(2, 3)` 을 Rust 로 짜고, Next.js 화면에 `5` 를 찍는다.
> 내용물은 하찮지만 **빌드 → 번들 → 로드** 경로 전체가 여기서 한 번 뚫린다.

## 전체 흐름

```
crates/core/src/lib.rs        pub fn add(a, b) -> u32          ← 순수 Rust (로직)
        │
crates/wasm/src/lib.rs        #[wasm_bindgen] pub fn add(...)  ← JS 에 노출 (껍데기)
        │  wasm-pack build --target web
        ▼
pkg/pixelvault_bg.wasm        컴파일된 WebAssembly 바이너리
pkg/pixelvault.js             wasm 을 불러오고 JS↔wasm 값 변환을 해 주는 "glue" 코드
pkg/pixelvault.d.ts           TypeScript 타입 (자동 생성!)
        │  www/package.json: "pixelvault-wasm": "file:../pkg"
        ▼
www/app/page.tsx              const wasm = await import("pixelvault-wasm"); await wasm.default(); wasm.add(2, 3)
```

## 핵심 개념

### 1. `#[wasm_bindgen]` — Rust 함수를 JS 에 노출하는 표시

```rust
#[wasm_bindgen]
pub fn add(a: u32, b: u32) -> u32 {
    pixelvault_core::add(a, b)
}
```

`#[...]` 는 **attribute(속성)** 라고 부른다. TS 의 데코레이터처럼 코드에 붙어서 컴파일러/매크로에게 지시를 준다.
`wasm_bindgen` 매크로는 이 함수를 wasm export 로 내보내고, JS 쪽 glue 코드와 `.d.ts` 타입을 생성하기 위한 정보를 심는다.

- Rust `u32` ↔ JS `number` 처럼 단순 타입은 그대로 넘어간다.
- 문자열, 배열, 구조체는 wasm-bindgen 이 메모리 복사/변환 코드를 만들어 준다. (M1 에서 다룸)

### 2. wasm-pack `--target` 선택

| target | 로드 방식 | 쓰는 곳 |
|---|---|---|
| `bundler` (기본값) | `import` 하면 번들러가 wasm 을 알아서 처리 | webpack 설정이 wasm 을 지원할 때 |
| **`web`** ✅ | ES 모듈 + **`init()` 을 직접 호출**해서 wasm 을 fetch | 번들러 설정 없이 어디서나 |
| `nodejs` | `require` 로 동기 로드 | Node 전용 |

Next.js 는 webpack/Turbopack 설정 차이 때문에 `bundler` 타깃이 자주 말썽을 부린다.
`--target web` 은 wasm 파일을 `new URL('pixelvault_bg.wasm', import.meta.url)` 로 가리키는데,
이 문법은 webpack 5 와 Turbopack 모두 "정적 파일로 복사해 줘"로 이해하므로 **추가 설정 없이 동작**한다.

### 3. 왜 동적 `import()` 인가

```tsx
useEffect(() => {
  (async () => {
    const wasm = await import("pixelvault-wasm"); // ① 브라우저에서만 모듈 로드
    await wasm.default();                          // ② .wasm 을 fetch + WebAssembly.instantiate
    setResult(wasm.add(2, 3));                     // ③ 이제 호출 가능
  })();
}, []);
```

> M1 이후로 이 코드는 `www/lib/pixelvault.ts` 의 `loadWasm()` 으로 옮겨졌다(한 번만 로드하도록 Promise 캐싱).

- Next.js 는 컴포넌트를 서버에서도 렌더링한다(SSR). 서버에는 wasm 을 fetch 할 브라우저가 없으니, `useEffect`(브라우저에서만 실행) 안에서 동적으로 불러온다.
- `init()`(= `wasm.default()`) 를 부르기 전에 `add` 를 호출하면 에러가 난다. wasm 인스턴스가 아직 없기 때문.

### 4. `next.config.ts` 의 `turbopack.root`

`www/` 는 `../pkg` 를 참조한다(프로젝트 루트 **바깥**). Turbopack 은 기본적으로 프로젝트 폴더 밖 파일을 보지 않으므로
레포 루트를 root 로 알려준다.

## 확인 방법

```bash
./scripts/build-wasm.sh
cd www && npm install && npm run dev
# http://localhost:3000 → "add(2, 3) = 5"
```

## 삽질 기록

- **wasm-pack 첫 실행이 1분 넘게 걸림**: `wasm-bindgen-cli` 와 `wasm-opt` 를 처음 한 번 다운로드/설치하기 때문. 정상.
- **"License key is set in Cargo.toml but no LICENSE file(s) were found"**: 경고일 뿐. 배포(M5) 전에 LICENSE 파일 추가.
