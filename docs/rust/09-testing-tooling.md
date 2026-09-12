# 09. 테스트와 도구

## 1. 테스트는 코드 옆에 둔다

```rust
// crates/core/src/resize.rs 맨 아래
#[cfg(test)]                // 테스트 빌드에서만 컴파일 → 배포물에 안 들어감
mod tests {
    use super::*;           // 부모 모듈(=이 파일)의 것들을 전부 가져온다

    #[test]
    fn fit_keeps_aspect_ratio() {
        assert_eq!(fit_within(4000, 3000, Some(1920), None), (1920, 1440));
    }
}
```

- **비공개 함수도 테스트할 수 있다**(같은 모듈 안이니까). TS 에서 export 만 위해 공개하던 함수를 감출 수 있다.
- `tests/` 폴더는 "통합 테스트"(공개 API 만 사용) 자리다. 이 레포에서는 픽스처 파일 보관에만 쓴다.

## 2. assert 매크로

```rust
assert!(cond);                               // 참이어야 함
assert!(cond, "메시지 {값:?}");              // 실패 시 출력할 메시지
assert_eq!(a, b);                            // 같아야 함 (실패하면 양쪽 값을 Debug 로 출력)
assert_ne!(a, b);
assert!(matches!(err, PixelVaultError::TooLarge { width: 640, .. }));   // enum 모양 검사
```

실패 메시지에 값을 찍으려면 그 타입에 `Debug` 가 필요하다 → `#[derive(Debug)]`.

이 레포에서 자주 쓴 패턴:

```rust
// 반복되는 검사에 라벨 붙이기 (어느 포맷에서 실패했는지 알 수 있게)
for format in ALL_FORMATS {
    let out = process(IPHONE, &ProcessOptions { format, ..Default::default() }).unwrap();
    assert_eq!((out.width, out.height), (300, 400), "{format}: 세로 사진이어야 함");
}

// 실패를 기대하는 테스트
let err = decode(b"definitely not an image").unwrap_err();
assert!(matches!(err, PixelVaultError::UnsupportedInput));
```

## 3. 픽스처: `include_bytes!`

```rust
const JPEG: &[u8] = include_bytes!("../tests/fixtures/landscape.jpg");
```

컴파일 시점에 파일 내용을 바이너리에 박아 넣는다. 런타임 파일 읽기가 없어서 **빠르고, 경로 문제가 없다**.

픽스처는 `examples/gen_fixtures.rs` 가 **코드로 생성**한다. 바이너리를 그냥 커밋해 두면
"이 이미지가 정확히 뭐였지?"를 나중에 알 수 없다. 생성 코드가 있으면 언제든 재현·수정할 수 있다.

## 4. 좋은 테스트를 만든 사례 (이 레포에서 버그를 잡은 것들)

```rust
// ① 오버플로 검사가 스스로 오버플로하던 버그
assert!(check_decoded_size(u32::MAX, u32::MAX, DEFAULT_MAX_DECODED_BYTES).is_err());
// → u64 곱셈도 넘쳐서 검사를 통과해 버렸다. saturating_mul 로 수정.

// ② "복사가 없었다"를 주소로 검증
let ptr_before = img.as_bytes().as_ptr();
let out = resize(img, Some(1920), None).unwrap();
assert_eq!(out.as_bytes().as_ptr(), ptr_before);

// ③ 알고리즘에 대한 내 가정이 틀렸음을 알려준 테스트
// 단색 이미지의 BlurHash 를 디코드하면 모든 픽셀이 같은 색일 거라 가정했지만,
// AC 성분 양자화 때문에 ±15 흔들린다. → 평균색 비교로 수정.

// ④ 실제 동작을 문서화하는 테스트
fn tolerates_truncated_jpeg_body() { /* 잘린 JPEG 도 디코더가 열어 준다 */ }
```

## 5. 실행

```bash
cargo test                      # 워크스페이스 전체
cargo test -p pixelvault-core   # 크레이트 지정
cargo test blurhash             # 이름에 blurhash 가 들어간 테스트만
cargo test -- --nocapture       # println! 출력 보기 (기본은 성공 시 감춤)
cargo test -- --test-threads=1  # 병렬 실행 끄기
```

## 6. wasm 런타임 테스트

```rust
// crates/wasm/tests/node.rs
use wasm_bindgen_test::*;

#[wasm_bindgen_test]
fn alpha_webp_exercises_libc_shim() { ... }
```

```bash
./scripts/test-wasm.sh      # wasm-pack test --node (+ llvm-ar 환경변수)
```

**역할 분담**이 핵심이다:
- 로직 검증 = 네이티브 `cargo test` (빠르고, 디버거·`println!`이 다 된다)
- wasm 테스트 = "wasm 으로 빌드해도 같은가"만 확인 (특히 C 링크, libc 셈, 타입 변환)

## 7. clippy — 린터

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

이 레포에서 실제로 잡힌 것들:

| 린트 | 내용 | 고친 방법 |
|---|---|---|
| `missing_safety_doc` | `pub unsafe fn` 에 `# Safety` 절이 없음 | 안전 조건을 문서로 명시 |
| `doc_list_item` | 문서 주석의 목록 들여쓰기가 애매함 | 번호 목록 형식으로 정리 |
| `needless_range_loop`, `redundant_clone` 등 | 더 관용적인 코드 제안 | 제안대로 수정 |

`-D warnings` 는 경고를 에러로 만든다. CI 에 걸어 두면 경고가 쌓이지 않는다.
특정 린트를 끄려면 `#[allow(clippy::some_lint)]` 를 그 항목에 붙이고, **왜 끄는지 주석을 남긴다.**

## 8. rustfmt — 포매터

```bash
cargo fmt            # 전체 포맷
cargo fmt --check    # CI 용: 포맷이 다르면 실패
```

Prettier 처럼 논쟁을 끝내 주는 도구다. 설정 없이 기본값을 쓰는 게 관례.

## 9. 문서 생성

```bash
cargo doc --open --no-deps
```

`///` 와 `//!` 주석이 HTML 문서가 된다. 문서 안의 코드 블록은 **doctest 로 실행**된다(```` ```rust ````).
실행되면 곤란한 예시는 ```` ```text ```` 나 ```` ```ignore ```` 로 표시한다 —
`webp_meta.rs` 의 파일 구조 다이어그램이 ```` ```text ```` 인 이유다.

## 10. 크기·의존성 분석

```bash
twiggy top -n 30 pkg/pixelvault_bg.wasm   # wasm 안에서 뭐가 큰지
cargo tree                                 # 의존성 트리
cargo tree -i <크레이트>                   # 누가 이 크레이트를 끌고 오는지(역추적)
```

`twiggy` 가 없었다면 "512 KB 짜리 16비트 알파 테이블이 링크되어 있다"는 걸 영영 몰랐을 것이다(→ 11 문서).

## 11. CI 에 무엇을 거는가

`.github/workflows/ci.yml`:

```
rust 잡:  cargo fmt --check → clippy -D warnings → cargo test
web  잡:  wasm32 clippy → build-wasm.sh(링크 검증 + 크기 예산) → test-wasm.sh
          → npm 패키지 빌드 + pack 검사 → 데모 사이트 lint + build
```

핵심은 **완료 기준(DoD) 중 기계가 검증할 수 있는 것을 전부 건다**는 점이다.
"wasm gzip 500KB 이하"도 `scripts/build-wasm.sh` 안에서 검사해서, 넘으면 빌드가 실패한다.

## 다음

→ [10. 코드 전체 해설](./10-code-walkthrough.md)
