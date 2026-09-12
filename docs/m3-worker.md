# M3 — Web Worker + 병렬 처리

> 목표: 무거운 wasm 호출을 메인 스레드에서 빼서 **UI 가 절대 멈추지 않게**, 그리고 여러 장을 **병렬로** 처리한다.

## 왜 필요한가

wasm 함수 호출은 **동기**다. `processImageRaw()` 가 도는 2~3초 동안 그 스레드는 다른 일을 전혀 못 한다.
메인 스레드에서 부르면 스크롤·클릭·애니메이션이 전부 멈춘다. (M1 데모에서 "변환 중…" 버튼이 늦게 그려지던 이유)

측정 결과 (Chrome, 5장 = 12MP 6.3MB + 24MP 9.4MB + 작은 것 3장):

| 실행 방식 | 전체 시간 | 메인 스레드 최대 멈춤 |
|---|---:|---:|
| 메인 스레드 | 9.7 s | **3,087 ms** |
| Worker 순차 (1개씩) | 5.1 s | **16 ms** |
| Worker 병렬 (4 workers) | 3.4 s | **34 ms** |

"메인 스레드 최대 멈춤"은 `MessageChannel` 로 메시지를 계속 주고받으며 **이벤트 사이의 최대 간격**을 잰 값이다.
(백그라운드 탭에서도 스로틀링을 받지 않는 방법. DevTools Performance 탭의 Long Task 와 같은 걸 본다)

## 구조: `packages/pixelvault` (npm 패키지)

M3 부터 TS 래퍼를 별도 패키지로 뺐다. 데모(`www`)는 이 패키지를 `file:../packages/pixelvault` 로 쓰고,
M5 에서 그대로 npm 에 배포한다.

```
packages/pixelvault/src/
├── types.ts    ProcessOptions / ProcessResult 등 공개 타입
├── core.ts     wasm 로드 + 옵션 검증 + 결과 정리 (메인 스레드·워커 공용)
├── worker.ts   워커 진입점. comlink 로 api 를 expose
├── pool.ts     워커 풀 (N 개 워커에 작업 분배)
└── index.ts    공개 API: processImage(메인 스레드), createPixelVault(워커 풀)
```

```ts
import { createPixelVault } from "@archibald1948/pixelvault";

const vault = createPixelVault();  // 워커 = min(4, 코어 수 - 1)
const result = await vault.process(file, { format: "webp", maxWidth: 1920 });

await vault.processMany(files, { format: "webp" }, {
  concurrency: 1,                  // 1 = 순차, 기본 = 워커 수(병렬)
  onProgress: ({ done, total, item }) => console.log(`${done}/${total}`),
});
```

## 핵심 개념

### 1. comlink — postMessage 를 함수 호출처럼

Worker 와의 통신은 원래 `postMessage` + `onmessage` 로 메시지를 주고받는 방식이라 요청/응답 짝을 직접 맞춰야 한다.
comlink 는 이걸 감춰서 워커의 객체를 **그냥 async 함수처럼** 부르게 해 준다:

```ts
// worker.ts
const api = { async process(input, options) { ... } };
Comlink.expose(api);

// pool.ts
const remote = Comlink.wrap<WorkerApi>(new Worker(...));
const result = await remote.process(file, options);   // ← 실제로는 postMessage 왕복
```

`WorkerApi = typeof api` 를 export 해서 메인 스레드 쪽도 **타입이 그대로 따라온다.**

### 2. 데이터를 스레드 사이로 옮기는 3가지 방법

| 방법 | 비용 | 이 프로젝트에서 |
|---|---|---|
| 구조화 복제(structured clone) — 기본 | 바이트 **복사** | 옵션 객체 같은 작은 값 |
| **Transfer** — `Comlink.transfer(obj, [buffer])` | 복사 0, 원본은 비워짐(detached) | 결과 `bytes` 를 워커 → 메인으로 |
| **Blob/File 전달** | 복사 0 (핸들만 전달) | 입력 파일 메인 → 워커 |

**입력은 File 을 그대로 넘긴다.** `File` 은 디스크 데이터를 가리키는 핸들이라 postMessage 해도 바이트가 복사되지 않는다.
워커가 `await file.arrayBuffer()` 로 **워커 스레드에서** 읽는다. 10 MB 파일을 읽는 시간조차 메인 스레드와 무관해진다.

**결과는 transfer 한다.** 워커가 만든 `Uint8Array` 의 버퍼 소유권을 메인 스레드로 넘기면 복사 없이 포인터만 이동하고,
워커 쪽 배열은 길이 0 이 된다. — Rust 의 **move** 와 똑같은 개념이 JS 에도 있다!

```ts
return Comlink.transfer(result, [result.bytes.buffer]);
```

### 3. 워커 풀 — 왜 워커마다 wasm 이 따로 있나

```
메인 스레드 ──▶ WorkerPool ─┬─▶ worker 0 (wasm 인스턴스 + 자기 메모리)
                             ├─▶ worker 1 (wasm 인스턴스 + 자기 메모리)
                             ├─▶ worker 2 ...
                             └─▶ worker 3
```

- 각 워커는 독립된 wasm 인스턴스와 선형 메모리를 가진다. 서로 메모리를 **공유하지 않는다.**
- 그래서 **`SharedArrayBuffer` 가 필요 없고**, 그걸 쓰기 위한 `COOP/COEP` 헤더(스펙 함정 목록 3번)도 필요 없다.
  COOP/COEP 는 켜면 외부 이미지/iframe/광고 임베드가 깨지는 등 부작용이 커서, 안 써도 되면 안 쓰는 게 낫다.
- 대가: 워커 수 × 메모리. 큰 사진 한 장 처리 피크가 수백 MB 라서 기본 워커 수를 **최대 4** 로 제한했다.
- 한 장을 여러 스레드로 쪼개는 병렬화(rayon + wasm threads)는 SharedArrayBuffer 가 필요하다. 이 프로젝트는
  "여러 장을 동시에"로 병렬성을 얻는다. 업로드 시나리오에서는 보통 파일이 여러 개라 이게 더 실용적이다.

풀 구현(`pool.ts`)은 단순하다: 작업 큐 + 놀고 있는 워커 목록. 워커는 **처음 필요할 때 생성**하고(`#spawn`),
작업이 끝나면 idle 목록에 돌려놓고 큐를 다시 확인한다(`#pump`).

### 4. 번들러가 워커를 찾게 하는 마법의 문법

```ts
new Worker(new URL("./worker.js", import.meta.url), { type: "module" })
```

webpack 5 / Vite / Turbopack 은 이 **정확한 모양**을 보면 `worker.js` 와 그 의존성(wasm glue, `.wasm`)을
별도 번들로 만들어 준다. 변수에 URL 을 담아서 넘기는 식으로 모양을 바꾸면 번들러가 인식하지 못한다.
Next.js 빌드 결과(`.next/static/media/`)에 `worker.*.js` 와 `pixelvault_bg.*.wasm` 이 생기는 걸로 확인할 수 있다.

### 5. JS ↔ Rust 숫자 경계의 함정

wasm-bindgen 은 JS `number` 를 Rust `u8`/`u32` 로 넘길 때 **범위를 검사하지 않고 비트를 자른다.**

- `quality: 300` → u8 로 잘려서 **44**
- `maxWidth: -1` → u32 로 **4294967295**

조용히 이상한 결과가 나오는 게 가장 나쁘다. 그래서 `core.ts` 의 `validate()` 가 wasm 을 부르기 전에 정수·범위를 검사해서
`RangeError` 를 던진다. "타입이 맞아도 값의 범위는 경계에서 따로 지켜야 한다"는 교훈.

### 6. 진행률

wasm 호출 하나는 쪼갤 수 없는 동기 작업이라 "한 장 안에서의 진행률(30%…)"은 없다.
대신 `processMany` 가 한 장 끝날 때마다 `onProgress({ done, total, item })` 을 부른다.
결과 배열은 **입력 순서**를 유지하고, 한 장이 실패해도 나머지는 계속 처리한다(`{ ok: false, error }`).

```ts
// concurrency 개의 "러너"가 공유 카운터에서 다음 인덱스를 하나씩 가져간다
const runner = async () => {
  while (next < total) {
    const index = next++;            // JS 는 싱글 스레드라 이 증가가 경쟁 상태 없이 안전하다
    results[index] = await this.process(inputs[index], options) ...
  }
};
await Promise.all(Array.from({ length: concurrency }, runner));
```

## 데모 UI (www)

- 여러 파일 선택 → 표로 결과 비교, 행 클릭 시 미리보기
- **실행 방식**: Worker 병렬 / Worker 순차 / 메인 스레드(비교용)
- **메인 스레드 모니터**: 움직이는 공 + FPS + 최장 프레임 간격 + Long Task.
  메인 스레드 모드로 돌리면 공이 멈추고 빨갛게 변한다 → 워커의 효과를 눈으로 확인.

## 삽질 기록

- **React 19 lint 규칙**: `useEffect` 안에서 동기적으로 `setState` 하거나 렌더 중에 `ref.current` 를 쓰면 에러.
  → 브라우저 전용 값은 `useSyncExternalStore`, 비동기 콜백 안에서 setState, ref 갱신은 effect 로.
- **백그라운드 탭 측정**: 탭이 가려지면 `requestAnimationFrame` 이 멈추고 `setTimeout` 이 1초(심하면 1분) 단위로 묶인다.
  Long Task 도 보고되지 않는다. 벤치마크는 **탭을 앞에 띄운 상태**로 해야 하고, 자동화 측정에는 `MessageChannel` 을 썼다.
