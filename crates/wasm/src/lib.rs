//! pixelvault-wasm — `pixelvault-core` 를 JS 에서 부를 수 있게 해 주는 바인딩.
//!
//! 규칙: 여기에는 로직을 두지 않는다. JS 타입 ↔ Rust 타입 변환만 한다.
//! 사람이 쓰기 좋은 API(`processImage(input, options)`)는 JS/TS 래퍼가 이 저수준 함수를 감싸서 제공한다.

use pixelvault_core::{OutputFormat, ProcessOptions};
use wasm_bindgen::prelude::*;

/// JS 에서 `add(2, 3)` 으로 호출된다. 실제 계산은 core 가 한다. (M0 헬로월드)
#[wasm_bindgen]
pub fn add(a: u32, b: u32) -> u32 {
    pixelvault_core::add(a, b)
}

/// 처리 결과. JS 쪽에서는 클래스 인스턴스로 보이고, 필드는 getter 로 읽는다.
///
/// 이 객체의 실제 데이터는 wasm 메모리 안에 있고 JS 는 포인터만 들고 있다.
/// 그래서 다 쓰고 나면 JS 에서 `.free()` 를 불러 wasm 쪽 메모리를 돌려줘야 한다.
#[wasm_bindgen]
pub struct RawProcessResult {
    bytes: Vec<u8>,
    width: u32,
    height: u32,
    format: OutputFormat,
    original_bytes: usize,
    output_bytes: usize,
    blurhash: Option<String>,
    source_orientation: u8,
    had_gps: bool,
    exif_kept: bool,
}

#[wasm_bindgen]
impl RawProcessResult {
    #[wasm_bindgen(getter)]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[wasm_bindgen(getter)]
    pub fn height(&self) -> u32 {
        self.height
    }

    #[wasm_bindgen(getter)]
    pub fn format(&self) -> String {
        self.format.as_str().to_owned()
    }

    #[wasm_bindgen(getter, js_name = mimeType)]
    pub fn mime_type(&self) -> String {
        self.format.mime_type().to_owned()
    }

    #[wasm_bindgen(getter, js_name = originalBytes)]
    pub fn original_bytes(&self) -> usize {
        self.original_bytes
    }

    #[wasm_bindgen(getter, js_name = outputBytes)]
    pub fn output_bytes(&self) -> usize {
        self.output_bytes
    }

    /// `Option<String>` 은 JS 에서 `string | undefined` 가 된다.
    #[wasm_bindgen(getter)]
    pub fn blurhash(&self) -> Option<String> {
        self.blurhash.clone()
    }

    #[wasm_bindgen(getter, js_name = sourceOrientation)]
    pub fn source_orientation(&self) -> u8 {
        self.source_orientation
    }

    #[wasm_bindgen(getter, js_name = hadGps)]
    pub fn had_gps(&self) -> bool {
        self.had_gps
    }

    #[wasm_bindgen(getter, js_name = exifKept)]
    pub fn exif_kept(&self) -> bool {
        self.exif_kept
    }

    /// 결과 바이트를 **꺼내 간다**(한 번만 호출 가능, 두 번째부터는 빈 배열).
    ///
    /// getter 로 `Vec<u8>` 을 돌려주면 읽을 때마다 `clone()` 이 필요하다.
    /// `std::mem::take` 는 필드를 빈 Vec 로 바꿔치기하면서 원래 Vec 의 소유권을 가져온다 → 복사 0회.
    /// 이후 wasm-bindgen 이 이 Vec 를 JS `Uint8Array` 로 한 번 복사하고, wasm 쪽 메모리는 해제된다.
    #[wasm_bindgen(js_name = takeBytes)]
    pub fn take_bytes(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.bytes)
    }
}

/// 저수준 처리 함수. 옵션을 평평한 인자로 받는다(객체를 넘기려면 serde 가 필요해서).
///
/// - `input: &[u8]` — JS 의 `Uint8Array` 가 wasm 메모리로 **한 번 복사**된 뒤, 그 영역을 빌려서 받는다.
/// - `Option<u32>` — JS 에서 `undefined` 를 넘기면 `None` 이 된다.
/// - 반환 `Result<_, JsError>` — `Err` 면 JS 에서 `Error` 가 throw 된다.
///   core 의 `PixelVaultError` 는 `std::error::Error` 를 구현하므로 `?` 가 `JsError` 로 자동 변환해 준다.
#[wasm_bindgen(js_name = processImageRaw)]
pub fn process_image_raw(
    input: &[u8],
    max_width: Option<u32>,
    max_height: Option<u32>,
    format: &str,
    quality: u8,
    strip_exif: bool,
    blurhash: bool,
) -> Result<RawProcessResult, JsError> {
    let options = ProcessOptions {
        max_width,
        max_height,
        format: format.parse()?,
        quality,
        strip_exif,
        blurhash,
    };

    let out = pixelvault_core::process(input, &options)?;

    Ok(RawProcessResult {
        bytes: out.bytes,
        width: out.width,
        height: out.height,
        format: out.format,
        original_bytes: out.original_bytes,
        output_bytes: out.output_bytes,
        blurhash: out.blurhash,
        source_orientation: out.source_orientation,
        had_gps: out.had_gps,
        exif_kept: out.exif_kept,
    })
}

/// BlurHash 문자열을 `width × height` RGBA 픽셀로 풀어 준다.
/// JS 에서 `new ImageData(new Uint8ClampedArray(pixels), width, height)` 로 캔버스에 그리면 된다.
#[wasm_bindgen(js_name = decodeBlurhash)]
pub fn decode_blurhash(hash: &str, width: u32, height: u32) -> Result<Vec<u8>, JsError> {
    Ok(pixelvault_core::blurhash::decode(hash, width, height)?)
}
