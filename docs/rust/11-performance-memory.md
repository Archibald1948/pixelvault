# 11. 성능과 메모리 — 측정한 것들

추측 대신 측정한 내용만 적는다. 방법론은 [../m4-benchmark.md](../m4-benchmark.md).

## 1. 릴리스 빌드는 필수

`cargo build` (dev 프로필)는 최적화가 꺼져 있어 이미지 처리 코드가 **10배 이상** 느리다.
벤치마크·데모는 항상 `--release`. `scripts/build-wasm.sh` 도 기본이 release 다.

dev 와 release 의 또 다른 차이: **정수 오버플로 검사**. dev 는 panic, release 는 조용히 wrap-around 한다.
그래서 크기 검사 같은 코드는 `checked_*`/`saturating_*` 로 명시적으로 처리해야 한다(→ 04, 07 문서).

## 2. `opt-level` 실측 (12MP JPEG → 1920px, Node, 중앙값)

| 설정 | wasm | gzip | → WebP | → JPEG | → PNG |
|---|---:|---:|---:|---:|---:|
| **opt-level=3, LTO** ✅ | 985 KB | **378 KB** | **605 ms** | **282 ms** | **583 ms** |
| opt-level="s", LTO | 759 KB | 310 KB | 782 ms | 377 ms | 806 ms |
| opt-level="z", LTO | 742 KB | 310 KB | 958 ms | 541 ms | 989 ms |
| opt-level=3, SIMD 끔 | 926 KB | 361 KB | 638 ms | 322 ms | 653 ms |

- `"z"` 는 인라이닝과 루프 펼치기를 포기해서 크기를 줄인다. **뜨거운 루프가 대부분인 코드에서는 손해가 크다.**
- 크기 예산(gzip 500 KB) 안에 들어오는 한, 속도를 택하는 게 맞았다.
- SIMD128 은 전체 파이프라인 기준 ~10% (리사이즈 단계만 보면 훨씬 크다).

`lto = true` 는 크레이트 경계를 넘어 인라이닝하고 죽은 코드를 지운다. `codegen-units = 1` 은 병렬 컴파일을 포기하는 대신
최적화 기회를 최대로 만든다. 둘 다 빌드는 느려지고 결과물은 좋아진다.

## 3. wasm 크기: 어디서 오는가

`twiggy top` 으로 본 초기 상태:

```
645626 (37%)  data segment ".rodata"      ← 정적 데이터
...
524288        fast_image_resize::alpha::common::RECIP_ALPHA16   ← 512 KB 짜리 테이블 하나!
```

`DynamicImage`(런타임 픽셀 타입 분기)를 라이브러리에 넘기면 **모든 픽셀 타입의 코드와 테이블**이 링크된다.
제네릭으로 `U8x3`/`U8x4` 만 인스턴스화하도록 바꾸자 **1.74 MB → 0.98 MB**.

크기를 줄이는 수단 정리:

| 수단 | 효과 |
|---|---|
| 의존성의 `default-features = false` + 필요한 feature 만 | 큼 (image: GIF/TIFF/AVIF 제거) |
| 제네릭으로 타입 고정 (안 쓰는 코드 생성 자체를 막기) | 큼 (-750 KB) |
| `lto`, `codegen-units = 1` | 중간 |
| `opt-level = "s"/"z"` | 중간 (속도 손해) |
| `wasm-opt` (wasm-pack 이 자동 실행) | 중간 |
| `panic = "abort"` | wasm32 는 기본이 abort 라 효과 없음 |

## 4. 메모리: 최대 사용량 계산하기

12MP(4032×3024) JPEG 를 1920px WebP 로 바꿀 때 wasm 메모리에 동시에 올라가는 것:

```
입력 바이트        6.3 MB   (JS → wasm 복사본)
디코드된 RGB8     36.6 MB   (4032 × 3024 × 3)
리사이즈 결과      8.3 MB   (1920 × 1440 × 3)
인코딩 버퍼        ~0.1 MB
```

- `resize` 가 **소유권을 가져가기 때문에** 리사이즈 직후 36 MB 가 해제된다. 순서를 바꾸면 피크가 그만큼 올라간다.
- BlurHash 는 32px 사본만 만든다(`resized_copy`). 원본을 `clone()` 했다면 8 MB 를 헛되이 복사했을 것이다.
- wasm 선형 메모리는 **한 번 늘어나면 줄어들지 않는다**(`memory.grow` 만 있고 shrink 가 없다).
  워커를 4개로 제한한 이유이기도 하다: 큰 사진을 동시에 처리하면 워커마다 수백 MB 를 들고 있게 된다.

### 상한 검사

```rust
// 픽셀을 풀기 "전에" 헤더만 읽고 검사한다
let (width, height) = decoder.dimensions();
check_decoded_size(width, height, max_decoded_bytes)?;   // 가로 × 세로 × 4 > 1 GiB 면 거절
```

## 5. 복사를 줄이는 설계 체크리스트

1. 함수가 값을 읽기만 하면 `&T`, 새로 만들면 반환값, 다 쓰면 소유권을 받는다.
2. "안 바꿔도 되는 경우"를 먼저 처리한다 — `resize` 는 크기가 같으면 입력을 그대로 반환한다.
3. `Cow` 로 "필요할 때만 복사"를 표현한다.
4. 결과를 꺼낼 때 `clone()` 대신 `mem::take`.
5. 경계(JS↔wasm, 메인↔워커)를 넘는 횟수를 줄인다. 큰 버퍼는 한 번만.
6. `Vec::with_capacity` 로 재할당을 없앤다.

## 6. 브라우저에서 측정할 때 주의할 것

- **워밍업**: V8 은 wasm 을 처음엔 빠른 컴파일러(Liftoff)로 돌리고, 반복되면 최적화 컴파일러(TurboFan)로 다시 컴파일한다.
  첫 호출은 2~4배 느리다 → 측정에서 제외.
- **가려진 탭**: `document.hidden` 이면 rAF 가 멈추고 타이머가 묶이며, macOS 에서는 렌더러가 효율 코어로 밀려
  **같은 작업이 3~4배 느려진다.** 실제로 이 프로젝트에서 2.4초 vs 0.6초 차이를 겪었다.
- **중앙값**을 쓴다(평균은 튀는 값에 끌려간다).
- 메인 스레드 점유는 `MessageChannel` 왕복 간격으로 잰다(스로틀링 영향을 안 받는다).

## 7. 네이티브 vs wasm 성능 감각

같은 Rust 코드라도 wasm 에서는 보통 **1.2~3배 느리다**:

- SIMD 폭이 128비트로 제한(AVX2 는 256, NEON 은 128이지만 더 많은 명령)
- 런타임 CPU 기능 감지가 없어서 가장 낮은 공통분모로 컴파일
- 경계 검사가 더 자주 남는다
- 브라우저 코덱(C++ + 멀티스레드 + GPU)과 비교하면 격차가 더 벌어진다 → `docs/m4-benchmark.md`

**그래서 wasm 을 쓰는 이유는 "속도"가 아니라** 메인 스레드 회피, 브라우저 간 동일한 결과,
브라우저가 제공하지 않는 기능(손실 WebP in Safari, EXIF 제어, BlurHash) 쪽이다.

## 8. 더 해볼 만한 것

- `wasm-opt -O4` / `--converge` 로 추가 최적화 실험
- libwebp 의 `method` 파라미터(0~6)를 노출해 속도/용량 트레이드오프 선택지 제공
- 리사이즈 필터 선택(Bilinear/CatmullRom/Lanczos3)을 옵션으로 — 용량 vs 선명도
- wasm threads(SharedArrayBuffer + rayon)로 한 장을 여러 코어로. 단 COOP/COEP 헤더가 필요해진다.

## 다음

→ [10. 코드 전체 해설](./10-code-walkthrough.md) 로 돌아가 실제 코드를 읽어 보자.
