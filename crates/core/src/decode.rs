//! 1단계: 바이트 → 픽셀 (`&[u8]` → `DynamicImage`)

use std::io::Cursor;

use image::{DynamicImage, ImageDecoder, ImageError, ImageFormat, ImageReader, Limits};

use crate::error::{PixelVaultError, Result};

/// 디코드된 이미지가 차지할 수 있는 최대 메모리 (기본 1 GiB).
///
/// wasm32 는 포인터가 32비트라 주소 공간이 4 GiB 가 한계다. 그 안에 입력 바이트,
/// 디코드된 픽셀, 리사이즈 결과, 인코딩 결과가 동시에 올라가므로 넉넉히 1/4 로 잡았다.
/// (8000×6000 사진 = 4800만 픽셀 × 4바이트 ≈ 192 MB → 여유 있게 통과)
pub const DEFAULT_MAX_DECODED_BYTES: u64 = 1 << 30;

/// 디코드 결과. 이미지 픽셀과 함께 원본 포맷과 메타데이터 원본 바이트를 들고 다닌다.
#[derive(Debug)]
pub struct Decoded {
    pub image: DynamicImage,
    pub format: ImageFormat,
    /// ICC 색 프로파일 원본 바이트
    pub icc: Option<Vec<u8>>,
    /// EXIF 원본 바이트 (TIFF 구조, "II*\0" 또는 "MM\0*" 로 시작)
    pub exif: Option<Vec<u8>>,
}

/// 기본 메모리 한도로 디코드한다.
pub fn decode(input: &[u8]) -> Result<Decoded> {
    decode_with_limit(input, DEFAULT_MAX_DECODED_BYTES)
}

/// 입력 바이트를 디코드한다.
///
/// `input` 은 `&[u8]` (빌린 슬라이스) 다. 원본 바이트를 복사하지 않고 그대로 읽는다.
/// `Cursor` 는 슬라이스에 "현재 읽는 위치"만 붙여서 `Read + Seek` 로 만들어 주는 얇은 래퍼다.
pub fn decode_with_limit(input: &[u8], max_decoded_bytes: u64) -> Result<Decoded> {
    // 확장자가 아니라 파일 앞부분의 매직 바이트(JPEG 는 FF D8 FF …)로 포맷을 추측한다.
    let mut reader = ImageReader::new(Cursor::new(input))
        .with_guessed_format()
        .map_err(ImageError::from)?;

    let format = match reader.format() {
        Some(f @ (ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::WebP)) => f,
        _ => return Err(PixelVaultError::UnsupportedInput),
    };

    // image 크레이트 내부 할당에도 같은 상한을 건다 (이중 안전장치).
    let mut limits = Limits::default();
    limits.max_alloc = Some(max_decoded_bytes);
    reader.limits(limits);

    // 여기까지는 헤더만 읽었다. 픽셀을 풀기 "전에" 크기를 검사하는 게 핵심.
    let mut decoder = reader.into_decoder()?;
    let (width, height) = decoder.dimensions();
    check_decoded_size(width, height, max_decoded_bytes)?;

    // 메타데이터도 픽셀을 풀기 전에 꺼낸다 (from_decoder 가 decoder 의 소유권을 가져가므로).
    // 메타데이터가 깨져 있어도 이미지 변환은 계속한다 → 에러는 None 으로 흡수.
    let icc = decoder.icc_profile().ok().flatten();
    let exif = decoder.exif_metadata().ok().flatten();

    let image = DynamicImage::from_decoder(decoder)?;
    Ok(Decoded {
        image: to_8bit_rgb_or_rgba(image),
        format,
        icc,
        exif,
    })
}

/// `가로 × 세로 × 4` 바이트(RGBA 최악의 경우)가 한도를 넘는지 검사한다.
///
/// `u32 * u32` 는 쉽게 넘친다(65536×65536 = 2^32). 그래서 `u64` 로 넓혀서 곱하고,
/// 그래도 넘칠 수 있는 극단값(u32::MAX² × 4 > u64::MAX)은 `saturating_mul` 로 최댓값에 고정한다.
/// (릴리스 빌드에서 정수 오버플로는 panic 없이 조용히 wrap-around 되므로, 검사 로직이 뚫릴 수 있다)
pub fn check_decoded_size(width: u32, height: u32, limit_bytes: u64) -> Result<()> {
    let needed_bytes = u64::from(width)
        .saturating_mul(u64::from(height))
        .saturating_mul(4);
    if needed_bytes > limit_bytes {
        return Err(PixelVaultError::TooLarge {
            width,
            height,
            needed_bytes,
            limit_bytes,
        });
    }
    Ok(())
}

/// 이후 단계가 다룰 픽셀 형식을 두 가지(RGB8, RGBA8)로 통일한다.
///
/// `image: DynamicImage` 를 값으로(소유권째) 받는다. 이미 RGB8/RGBA8 이면 그대로 돌려주는데,
/// 이때 픽셀 버퍼는 복사되지 않고 소유권만 이동한다. 그 외(흑백, 16비트 PNG 등)만 변환한다.
pub fn to_8bit_rgb_or_rgba(image: DynamicImage) -> DynamicImage {
    match image {
        DynamicImage::ImageRgb8(_) | DynamicImage::ImageRgba8(_) => image,
        other if other.color().has_alpha() => DynamicImage::ImageRgba8(other.into_rgba8()),
        other => DynamicImage::ImageRgb8(other.into_rgb8()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::ColorType;

    const JPEG: &[u8] = include_bytes!("../tests/fixtures/landscape.jpg");
    const PNG_ALPHA: &[u8] = include_bytes!("../tests/fixtures/transparent.png");
    const PNG_GRAY16: &[u8] = include_bytes!("../tests/fixtures/gray16.png");
    const WEBP: &[u8] = include_bytes!("../tests/fixtures/photo.webp");

    #[test]
    fn decodes_jpeg_as_rgb8() {
        let d = decode(JPEG).unwrap();
        assert_eq!(d.format, ImageFormat::Jpeg);
        assert_eq!(d.image.color(), ColorType::Rgb8);
        assert_eq!((d.image.width(), d.image.height()), (640, 480));
    }

    #[test]
    fn decodes_png_with_alpha_as_rgba8() {
        let d = decode(PNG_ALPHA).unwrap();
        assert_eq!(d.format, ImageFormat::Png);
        assert_eq!(d.image.color(), ColorType::Rgba8);
    }

    #[test]
    fn normalizes_16bit_grayscale_to_rgb8() {
        let d = decode(PNG_GRAY16).unwrap();
        assert_eq!(d.image.color(), ColorType::Rgb8);
    }

    #[test]
    fn decodes_webp() {
        let d = decode(WEBP).unwrap();
        assert_eq!(d.format, ImageFormat::WebP);
        assert_eq!((d.image.width(), d.image.height()), (320, 240));
    }

    #[test]
    fn extracts_exif_from_jpeg() {
        let d = decode(include_bytes!("../tests/fixtures/iphone-portrait.jpg")).unwrap();
        let exif = d.exif.expect("fixture has EXIF");
        assert!(exif.starts_with(b"II*\0") || exif.starts_with(b"MM\0*"));
        assert!(decode(JPEG).unwrap().exif.is_none());
    }

    #[test]
    fn rejects_garbage() {
        let err = decode(b"definitely not an image").unwrap_err();
        assert!(matches!(err, PixelVaultError::UnsupportedInput));
    }

    #[test]
    fn rejects_corrupted_jpeg() {
        // 헤더 일부만 남긴 JPEG. 매직 바이트는 JPEG 라서 포맷 추측은 통과하지만 디코드는 실패해야 한다.
        let err = decode(&JPEG[..64]).unwrap_err();
        assert!(matches!(err, PixelVaultError::Codec(_)), "{err:?}");
    }

    #[test]
    fn tolerates_truncated_jpeg_body() {
        // 반대로 픽셀 데이터 중간에서 잘린 JPEG 는 디코더가 남은 부분을 채워서라도 열어 준다.
        // (브라우저도 똑같이 동작한다 — 다운로드 중인 JPEG 가 위에서부터 보이는 이유)
        let d = decode(&JPEG[..JPEG.len() / 2]).unwrap();
        assert_eq!((d.image.width(), d.image.height()), (640, 480));
    }

    #[test]
    fn enforces_memory_limit_before_decoding() {
        // 640×480×4 = 1,228,800 바이트 → 1 MB 한도면 거절되어야 한다
        let err = decode_with_limit(JPEG, 1_000_000).unwrap_err();
        assert!(matches!(
            err,
            PixelVaultError::TooLarge {
                width: 640,
                height: 480,
                needed_bytes: 1_228_800,
                ..
            }
        ));
    }

    #[test]
    fn size_check_does_not_overflow() {
        // u32 로 곱했으면 넘쳤을 크기. u64 로 계산하므로 정확히 거절된다.
        assert!(check_decoded_size(u32::MAX, u32::MAX, DEFAULT_MAX_DECODED_BYTES).is_err());
        assert!(check_decoded_size(8000, 6000, DEFAULT_MAX_DECODED_BYTES).is_ok());
    }
}
