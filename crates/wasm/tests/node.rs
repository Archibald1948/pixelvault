//! wasm 런타임 스모크 테스트. 로직 검증은 core 의 `cargo test` 가 하고,
//! 여기서는 "wasm 으로 빌드했을 때도 똑같이 동작하는가"(특히 libwebp + libc 셈)만 확인한다.
//!
//!     ./scripts/test-wasm.sh

use pixelvault_wasm::{decode_blurhash, process_image_raw};
use wasm_bindgen_test::*;

const JPEG: &[u8] = include_bytes!("../../core/tests/fixtures/landscape.jpg");
const PNG_ALPHA: &[u8] = include_bytes!("../../core/tests/fixtures/transparent.png");
const IPHONE: &[u8] = include_bytes!("../../core/tests/fixtures/iphone-portrait.jpg");

#[wasm_bindgen_test]
fn jpeg_to_webp_in_wasm() {
    let mut r = process_image_raw(JPEG, Some(320), None, "webp", 82, true, false).unwrap();
    assert_eq!((r.width(), r.height()), (320, 240));
    let bytes = r.take_bytes();
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WEBP");
    // 두 번째 take 는 빈 배열 (소유권은 이미 넘어갔다)
    assert!(r.take_bytes().is_empty());
}

#[wasm_bindgen_test]
fn alpha_webp_exercises_libc_shim() {
    // 알파가 있는 WebP 는 알파 평면을 무손실(VP8L)로 압축한다.
    // 이 경로가 qsort/bsearch/calloc 등 crates/wasm-libc 의 함수를 실제로 호출한다.
    let mut r = process_image_raw(PNG_ALPHA, None, None, "webp", 90, true, false).unwrap();
    let decoded = image::load_from_memory(&r.take_bytes()).unwrap();
    assert!(decoded.color().has_alpha());
    assert_eq!(
        decoded.to_rgba8().get_pixel(0, 0)[3],
        0,
        "모서리는 투명해야 함"
    );
    assert!(
        decoded.to_rgba8().get_pixel(128, 128)[3] > 150,
        "가운데는 불투명해야 함"
    );
}

#[wasm_bindgen_test]
fn errors_become_js_errors() {
    assert!(process_image_raw(b"nope", None, None, "webp", 82, true, false).is_err());
    assert!(process_image_raw(JPEG, None, None, "gif", 82, true, false).is_err());
}

#[wasm_bindgen_test]
fn iphone_portrait_in_wasm() {
    let r = process_image_raw(IPHONE, None, None, "webp", 82, true, true).unwrap();
    assert_eq!((r.width(), r.height()), (300, 400));
    assert_eq!(r.source_orientation(), 6);
    assert!(r.had_gps());
    assert!(!r.exif_kept());

    let hash = r.blurhash().expect("blurhash requested");
    let px = decode_blurhash(&hash, 3, 4).unwrap();
    assert_eq!(px.len(), 3 * 4 * 4);
}
