# M4 — 벤치마크: Canvas API vs WASM

> 목표: 같은 작업을 브라우저 내장 Canvas API(순수 JS)와 pixelvault(WASM)로 돌려서 **시간 / 용량 / 품질 / 메인 스레드 점유** 를 숫자로 비교한다.

- 페이지: `www/app/bench/page.tsx` → `/bench`
- 러너: `www/lib/bench.ts`, 품질 지표: `www/lib/metrics.ts`
- 자동화: `node bench/browser-bench.mjs` (헤드리스 Chrome 에서 페이지를 돌려 `bench/results/*.md` 저장)

## 결과 (헤드리스 Chrome 152, Apple Silicon 8코어, 12MP → 1920px, 품질 82, 5회 중앙값)

| 포맷 | 방식 | 시간 | 결과 크기 | SSIM* | 메인 스레드 최대 멈춤 |
|---|---|---:|---:|---:|---:|
| webp | Canvas API | **238 ms** | **87 KB** | 0.961 | 40 ms |
| webp | WASM (Worker) | 476 ms | 103 KB | 0.941 | **4 ms** |
| jpeg | Canvas API | **98 ms** | **201 KB** | 0.970 | 38 ms |
| jpeg | WASM (Worker) | 141 ms | 230 KB | 0.953 | **1 ms** |
| png | Canvas API | **168 ms** | 4,996 KB | 1.000 | 93 ms |
| png | WASM (Worker) | 757 ms | **3,871 KB** | 1.000 | **5 ms** |

\* 각 방식 자신의 무손실 리사이즈 결과 대비 (아래 "품질을 어떻게 비교하나" 참고)

## 솔직한 해석

### 1. 순수 속도는 Chrome 네이티브가 1.5~4배 빠르다

Chrome 의 Canvas 경로는 C++ 로 컴파일된 **libjpeg-turbo / libwebp 를 CPU 전용 SIMD(NEON/AVX2)로** 돌리고,
디코드는 별도 스레드, 리사이즈는 GPU 를 쓸 수도 있다. WASM 은 CPU 에 상관없이 동작하는 128비트 SIMD 만 쓸 수 있고,
libwebp 는 wasm SIMD 경로가 없어서 스칼라 코드로 돈다. "WASM 이라서 무조건 빠르다"는 틀린 기대다.

### 2. 대신 메인 스레드는 거의 안 막힌다

Canvas 도 디코드·인코드 일부는 비동기지만 `drawImage`(리사이즈)와 PNG 인코딩은 메인 스레드를 수십~백 ms 막는다(PNG 93 ms = Long Task).
WASM 은 파일 읽기부터 인코딩까지 **전부 워커에서** 하므로 1~5 ms. 여러 장을 올리면 차이가 누적된다(M3 측정: 5장에서 3초 vs 34 ms).

### 3. 용량 차이는 인코더가 아니라 **리사이즈 필터** 때문이다 — 실험으로 확인

WASM 결과가 더 큰 걸 보고 처음엔 인코더가 나쁜 줄 알았다. 그래서 **같은 픽셀** 을 두 인코더에 넣어 봤다:

| 입력 픽셀 | Canvas JPEG | WASM JPEG | Canvas WebP | WASM WebP |
|---|---:|---:|---:|---:|
| Canvas 로 리사이즈한 결과 | 159 KB | 161 KB | 55 KB | 55 KB |
| Lanczos3 로 리사이즈한 결과 | 180 KB | 183 KB | 62 KB | 62 KB |

같은 픽셀이면 **거의 똑같다** (WebP 는 둘 다 libwebp 라서 당연히 같고, JPEG 도 1% 차이).
차이는 리사이즈 필터에서 온다: Lanczos3 는 Canvas 의 부드러운 필터보다 **디테일(과 노이즈)을 더 살린다** → 인코더가 저장할 정보가 많다 → 크다.
벤치마크 페이지 아래의 3배 확대 비교에서 WASM 쪽 선이 더 또렷한 걸 볼 수 있다. 품질을 택하느냐 용량을 택하느냐의 문제.

### 4. 그 과정에서 찾은 진짜 개선점: JPEG 인코더 교체 (302 KB → 230 KB)

첫 측정에서 WASM JPEG 가 302 KB 로 Canvas(201 KB)보다 **50% 컸다.** 위 실험 전에 인코더부터 의심해서 조사해 보니
`image` 크레이트의 JPEG 인코더는 **4:2:2 서브샘플링 + 고정 허프만 테이블** 이었다.

`jpeg-encoder` 크레이트로 교체:
- **4:2:0 크로마 서브샘플링**: 색(Cb, Cr) 정보를 가로·세로 절반 해상도로 저장. 사람 눈은 밝기보다 색 변화에 둔감하다.
- **최적화 허프만 테이블**: 이 이미지에 실제로 나온 심볼 빈도로 테이블을 만든다(2-pass).

결과: **302 → 230 KB (−24%)**, 게다가 184 → 141 ms 로 더 빨라졌다. 교훈: *같은 "quality 82" 라도 인코더 구현에 따라 용량이 크게 다르다.*

### 5. WASM 이 이기는 지점

| | Canvas API | pixelvault |
|---|---|---|
| 메인 스레드 | 수십~백 ms 막힘 | 1~5 ms |
| 결과 일관성 | 브라우저마다 리사이즈 필터·인코더가 다름 | 모든 브라우저에서 바이트 단위로 같은 결과 |
| Safari 에서 WebP | ❌ (PNG 로 몰래 대체됨) | ✅ |
| EXIF | 무조건 전부 삭제 | 기본 삭제 + 선택적 유지(방향 태그 자동 초기화) |
| ICC 색 프로파일 | sRGB 로 변환 후 버림 (P3 사진 색이 줄어듦) | 보존 |
| PNG 용량 | 4.9 MB | 3.9 MB (−23%) |
| BlurHash | 별도 라이브러리 | 내장 |
| 추가 다운로드 | 0 | wasm ≈ 410 KB (gzip) |

## 측정 방법 (그리고 흔한 실수들)

### 워밍업 1회는 버린다

V8 은 wasm 을 처음에 **빠르게 컴파일하는 기본 컴파일러(Liftoff)** 로 돌리다가, 자주 쓰이면 **최적화 컴파일러(TurboFan)** 로 다시 컴파일한다(tier-up).
첫 호출은 느리므로 측정에서 뺀다. JS 쪽 JIT 도 마찬가지.

### 중앙값을 쓴다

평균은 GC·다른 탭 같은 우연한 튀는 값에 끌려간다. 중앙값이 "보통의 경우"를 더 잘 나타낸다.

### 가려진 탭에서 재면 안 된다

이 프로젝트에서 실제로 겪은 일: 브라우저 창이 다른 창 뒤에 있으면(`document.hidden === true`)
- `requestAnimationFrame` 이 멈추고,
- `setTimeout` 이 1초~1분 단위로 묶이고,
- 렌더러 프로세스의 CPU 우선순위가 내려가(macOS 에서는 효율 코어로) **같은 작업이 3~4배 느려졌다.**

그래서 README 숫자는 **헤드리스 Chrome**(`bench/browser-bench.mjs`)으로 뽑았다. 헤드리스는 "가려짐" 상태가 없다.
직접 잴 때는 탭을 앞에 띄워 두자.

### 메인 스레드 멈춤은 `MessageChannel` 로 잰다

```js
const ch = new MessageChannel();
ch.port1.onmessage = () => { maxGap = Math.max(maxGap, now - last); ...; ch.port2.postMessage(0); };
```

메시지를 계속 주고받으면서 **이벤트 사이 최대 간격** 을 기록한다. 메인 스레드가 막히면 메시지가 처리되지 못해 간격이 벌어진다.
`setTimeout` 은 최소 지연·스로틀링이 있어서 부정확하다. (측정기가 메인 스레드 코어 하나를 쓰므로 시간 측정과는 따로 돌린다)

### 품질을 어떻게 비교하나 — 기준 선택의 함정

"원본 대비 SSIM" 을 재려면 원본(4032px)과 결과(1920px)의 크기가 달라서 결국 **누군가의 리사이즈 결과** 를 기준으로 삼아야 한다.
Lanczos3 결과를 기준으로 하면 WASM 이, Canvas 결과를 기준으로 하면 Canvas 가 유리해진다.

그래서 두 가지로 나눴다:
- **SSIM (각자의 무손실 리사이즈 대비)**: "인코딩으로 잃은 품질". 인코더끼리 공정하게 비교.
- **3배 확대 크롭**: "리사이즈 필터의 선명도" 는 눈으로.

Canvas 쪽 SSIM 이 조금 높은 이유도 3번과 같다: 이미 부드럽게 뭉개진 이미지는 압축해도 잃을 디테일이 적다.

### SSIM 이란

두 이미지를 8×8 창으로 훑으면서 **밝기(평균)·대비(분산)·구조(공분산)** 가 얼마나 비슷한지 계산해 0~1 로 나타낸다.
픽셀 차이의 제곱 평균인 PSNR 보다 사람 눈의 판단과 잘 맞는다. `www/lib/metrics.ts` 에 60줄로 구현했다.

## 헤드리스 자동화 구조

`bench/browser-bench.mjs` 는 외부 라이브러리(puppeteer 등) 없이 동작한다:

1. 임시 프로필로 Chrome 을 `--headless=new --remote-debugging-port=9333` 로 띄운다 (평소 쓰는 브라우저와 분리)
2. `http://127.0.0.1:9333/json/list` 에서 페이지의 WebSocket 주소를 얻는다
3. Node 22 내장 `WebSocket` 으로 **Chrome DevTools Protocol** 명령(`Page.navigate`, `Runtime.evaluate`)을 보낸다
4. 페이지가 노출한 `window.__pixelvaultBench()` 를 호출해 결과 JSON 을 받는다

```bash
cd www && npm run build && npm start      # 터미널 1
RUNS=5 FORMATS=webp,jpeg,png node bench/browser-bench.mjs   # 터미널 2
```
