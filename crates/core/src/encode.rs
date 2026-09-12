//! 3단계: 픽셀 → 바이트 (`&DynamicImage` → `Vec<u8>`)

use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;

use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{DynamicImage, ImageEncoder, RgbImage, RgbaImage};

use crate::error::{PixelVaultError, Result};

/// 기본 품질. 82 는 눈으로 차이를 느끼기 어려우면서 용량이 크게 줄어드는 흔한 기본값이다.
pub const DEFAULT_QUALITY: u8 = 82;

/// libwebp 가 허용하는 한 변의 최대 길이
pub const WEBP_MAX_DIMENSION: u32 = 16383;

/// 출력 포맷. 문자열 대신 enum 을 쓰면 오타("wepb")가 컴파일/파싱 단계에서 걸러지고,
/// `match` 에서 모든 경우를 처리했는지 컴파일러가 검사해 준다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    #[default]
    Webp,
    Jpeg,
    Png,
}

impl OutputFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            OutputFormat::Webp => "webp",
            OutputFormat::Jpeg => "jpeg",
            OutputFormat::Png => "png",
        }
    }

    pub fn mime_type(self) -> &'static str {
        match self {
            OutputFormat::Webp => "image/webp",
            OutputFormat::Jpeg => "image/jpeg",
            OutputFormat::Png => "image/png",
        }
    }
}

impl fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// `"webp".parse::<OutputFormat>()` 처럼 문자열에서 변환할 수 있게 해 준다.
/// JS 에서 넘어오는 `format: 'webp' | 'jpeg' | 'png'` 를 받을 때 쓴다.
impl FromStr for OutputFormat {
    type Err = PixelVaultError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "webp" => Ok(OutputFormat::Webp),
            "jpeg" | "jpg" => Ok(OutputFormat::Jpeg),
            "png" => Ok(OutputFormat::Png),
            other => Err(PixelVaultError::InvalidOption(format!(
                "unknown format {other:?} (expected webp, jpeg or png)"
            ))),
        }
    }
}

/// JPEG APP1 세그먼트 하나에 들어갈 수 있는 EXIF 최대 크기 (65535 − 길이필드 2 − "Exif\0\0" 6)
pub const JPEG_MAX_EXIF: usize = 65527;

/// JPEG 는 가로·세로를 16비트로 저장한다
pub const JPEG_MAX_DIMENSION: u32 = 65535;

/// 결과 파일에 함께 넣을 메타데이터. 둘 다 원본 바이트를 **빌린다**(`&'a [u8]`).
///
/// - `icc`: 색 프로파일. 아이폰 사진(Display P3)처럼 sRGB 가 아닌 사진은 이게 없으면 색이 칙칙해진다.
///   개인정보가 아니라 "색을 어떻게 해석할지"에 대한 정보라 기본적으로 유지한다.
/// - `exif`: 촬영 정보/GPS. 개인정보라 기본적으로 **넣지 않는다**(stripExif).
#[derive(Debug, Clone, Copy, Default)]
pub struct Metadata<'a> {
    pub icc: Option<&'a [u8]>,
    pub exif: Option<&'a [u8]>,
}

/// 이미지를 지정한 포맷으로 인코딩한다 (메타데이터 없이).
///
/// `image` 를 `&DynamicImage` 로 **빌려서** 받는다. 인코딩은 픽셀을 읽기만 하면 되고,
/// 호출한 쪽(pipeline)은 인코딩 후에도 가로/세로 크기나 BlurHash 계산에 이미지를 계속 써야 하기 때문.
///
/// `quality` 는 1~100. PNG 는 무손실이라 무시한다.
pub fn encode(image: &DynamicImage, format: OutputFormat, quality: u8) -> Result<Vec<u8>> {
    encode_with_metadata(image, format, quality, Metadata::default())
}

/// 메타데이터(ICC/EXIF)를 함께 넣어 인코딩한다.
pub fn encode_with_metadata(
    image: &DynamicImage,
    format: OutputFormat,
    quality: u8,
    meta: Metadata<'_>,
) -> Result<Vec<u8>> {
    if !(1..=100).contains(&quality) {
        return Err(PixelVaultError::InvalidOption(format!(
            "quality must be 1-100, got {quality}"
        )));
    }

    let image = as_rgb8_or_rgba8(image);
    match format {
        OutputFormat::Webp => {
            let webp = encode_webp(&image, quality)?;
            // libwebp 간단 API 는 메타데이터를 못 넣으므로 RIFF 청크를 직접 조립한다
            crate::webp_meta::add_metadata(webp, image.width(), image.height(), meta.icc, meta.exif)
        }
        OutputFormat::Jpeg => encode_jpeg(&image, quality, meta),
        OutputFormat::Png => encode_png(&image, meta),
    }
}

/// image 크레이트 인코더(PNG 등)에 메타데이터를 설정한다. `ImageEncoder` 트레잇을 구현한 인코더라면
/// 무엇이든 받을 수 있는 제네릭 함수다.
fn attach_metadata<E: ImageEncoder>(encoder: &mut E, meta: Metadata<'_>) -> Result<()> {
    let unsupported = |e: image::error::UnsupportedError| {
        PixelVaultError::Codec(image::ImageError::Unsupported(e))
    };
    if let Some(icc) = meta.icc {
        encoder.set_icc_profile(icc.to_vec()).map_err(unsupported)?;
    }
    if let Some(exif) = meta.exif {
        encoder
            .set_exif_metadata(exif.to_vec())
            .map_err(unsupported)?;
    }
    Ok(())
}

/// 이미 RGB8/RGBA8 이면 빌린 채로(`Cow::Borrowed`, 복사 0), 아니면 변환해서 소유(`Cow::Owned`).
///
/// `Cow`(Clone-on-Write) 는 "대부분은 빌려 쓰고, 필요할 때만 새로 만든다"를 타입으로 표현한 것.
/// pipeline 을 거친 이미지는 항상 RGB8/RGBA8 이라 실제로는 거의 항상 Borrowed 경로를 탄다.
fn as_rgb8_or_rgba8(image: &DynamicImage) -> Cow<'_, DynamicImage> {
    match image {
        DynamicImage::ImageRgb8(_) | DynamicImage::ImageRgba8(_) => Cow::Borrowed(image),
        other => Cow::Owned(crate::decode::to_8bit_rgb_or_rgba(other.clone())),
    }
}

fn encode_webp(image: &DynamicImage, quality: u8) -> Result<Vec<u8>> {
    let (w, h) = (image.width(), image.height());
    if w > WEBP_MAX_DIMENSION || h > WEBP_MAX_DIMENSION {
        return Err(PixelVaultError::InvalidOption(format!(
            "WebP supports at most {WEBP_MAX_DIMENSION}px per side, got {w}x{h}"
        )));
    }

    // webp::Encoder 는 픽셀 슬라이스(&[u8])를 빌리기만 한다 — 여기서도 복사 없음.
    let encoder = match image {
        DynamicImage::ImageRgba8(buf) => webp::Encoder::from_rgba(buf.as_raw(), w, h),
        DynamicImage::ImageRgb8(buf) => webp::Encoder::from_rgb(buf.as_raw(), w, h),
        _ => unreachable!("as_rgb8_or_rgba8 guarantees RGB8/RGBA8"),
    };

    let memory = encoder
        .encode_simple(false, f32::from(quality))
        .map_err(|e| PixelVaultError::WebpEncode(format!("{e:?}")))?;

    // `memory` 는 libwebp(C)가 malloc 한 버퍼를 감싼 타입이고, drop 될 때 C 쪽 free 가 불린다.
    // Rust 가 소유하는 Vec<u8> 로 한 번 복사해서 돌려준다.
    Ok(memory.to_vec())
}

fn encode_jpeg(image: &DynamicImage, quality: u8, mut meta: Metadata<'_>) -> Result<Vec<u8>> {
    let (w, h) = (image.width(), image.height());
    if w > JPEG_MAX_DIMENSION || h > JPEG_MAX_DIMENSION {
        return Err(PixelVaultError::InvalidOption(format!(
            "JPEG supports at most {JPEG_MAX_DIMENSION}px per side, got {w}x{h}"
        )));
    }

    // JPEG 는 알파 채널이 없다. 그냥 버리면 투명 픽셀(보통 RGB=0)이 검게 나오므로 흰 배경에 합성한다.
    let flattened;
    let rgb: &RgbImage = match image {
        DynamicImage::ImageRgb8(buf) => buf,
        DynamicImage::ImageRgba8(buf) => {
            flattened = flatten_on_white(buf);
            &flattened
        }
        _ => unreachable!("as_rgb8_or_rgba8 guarantees RGB8/RGBA8"),
    };

    // JPEG 는 EXIF 를 64KB 세그먼트 하나에만 담을 수 있다. 넘치면(거대한 썸네일/MakerNote) 포기한다.
    if meta.exif.is_some_and(|e| e.len() > JPEG_MAX_EXIF) {
        meta.exif = None;
    }

    let mut out = Vec::new();
    let mut encoder = jpeg_encoder::Encoder::new(&mut out, quality);
    // 최적화 허프만 테이블: 이 이미지에 실제로 나온 심볼 빈도로 테이블을 만든다(2-pass). 5~10% 작아진다.
    // 크로마 서브샘플링은 기본값을 쓴다: 품질 < 90 이면 4:2:0 (색 정보를 가로·세로 절반 해상도로 저장).
    // 사람 눈은 밝기보다 색 변화에 둔감해서, 이것만으로 용량이 크게 준다. libjpeg-turbo(브라우저)도 4:2:0.
    encoder.set_optimized_huffman_tables(true);
    if let Some(icc) = meta.icc {
        encoder.add_icc_profile(icc)?;
    }
    if let Some(exif) = meta.exif {
        encoder.add_exif_metadata(exif)?;
    }
    // w, h 는 위에서 65535 이하임을 확인했으므로 u16 변환이 안전하다
    encoder.encode(
        rgb.as_raw(),
        w as u16,
        h as u16,
        jpeg_encoder::ColorType::Rgb,
    )?;
    Ok(out)
}

fn encode_png(image: &DynamicImage, meta: Metadata<'_>) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder =
        PngEncoder::new_with_quality(&mut out, CompressionType::Default, FilterType::Adaptive);
    attach_metadata(&mut encoder, meta)?;
    encoder.write_image(
        image.as_bytes(),
        image.width(),
        image.height(),
        image.color().into(),
    )?;
    Ok(out)
}

/// RGBA → RGB, 흰 배경 위에 알파 합성: `out = c·a + 255·(1−a)`
fn flatten_on_white(rgba: &RgbaImage) -> RgbImage {
    let mut out = RgbImage::new(rgba.width(), rgba.height());
    // chunks_exact(4): 픽셀 단위로 4바이트씩 잘라 보는 "뷰". 새 메모리를 만들지 않는다.
    for (src, dst) in rgba.as_raw().chunks_exact(4).zip(out.chunks_exact_mut(3)) {
        let a = u16::from(src[3]);
        for c in 0..3 {
            let v = u16::from(src[c]) * a + 255 * (255 - a);
            dst[c] = ((v + 127) / 255) as u8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GenericImageView, ImageFormat, Rgba};

    fn sample_rgb() -> DynamicImage {
        let buf = RgbImage::from_fn(64, 48, |x, y| {
            image::Rgb([(x * 4) as u8, (y * 5) as u8, 128])
        });
        DynamicImage::ImageRgb8(buf)
    }

    fn transparent_rgba() -> DynamicImage {
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(8, 8, Rgba([0, 0, 0, 0])))
    }

    #[test]
    fn parses_format_strings() {
        assert_eq!("webp".parse::<OutputFormat>().unwrap(), OutputFormat::Webp);
        assert_eq!("JPG".parse::<OutputFormat>().unwrap(), OutputFormat::Jpeg);
        assert!("gif".parse::<OutputFormat>().is_err());
    }

    #[test]
    fn rejects_quality_out_of_range() {
        assert!(encode(&sample_rgb(), OutputFormat::Jpeg, 0).is_err());
        assert!(encode(&sample_rgb(), OutputFormat::Jpeg, 101).is_err());
    }

    #[test]
    fn each_format_round_trips() {
        let src = sample_rgb();
        for (fmt, expected) in [
            (OutputFormat::Webp, ImageFormat::WebP),
            (OutputFormat::Jpeg, ImageFormat::Jpeg),
            (OutputFormat::Png, ImageFormat::Png),
        ] {
            let bytes = encode(&src, fmt, 80).unwrap();
            assert_eq!(image::guess_format(&bytes).unwrap(), expected, "{fmt}");
            let back = image::load_from_memory(&bytes).unwrap();
            assert_eq!(back.dimensions(), (64, 48), "{fmt}");
        }
    }

    #[test]
    fn webp_quality_affects_size() {
        // 노이즈가 많은 이미지일수록 품질에 따른 용량 차이가 확실하다
        let noisy = RgbImage::from_fn(128, 128, |x, y| {
            let v = (x.wrapping_mul(2654435761) ^ y.wrapping_mul(40503)) as u8;
            image::Rgb([v, v.rotate_left(3), v.rotate_left(5)])
        });
        let img = DynamicImage::ImageRgb8(noisy);
        let low = encode(&img, OutputFormat::Webp, 10).unwrap();
        let high = encode(&img, OutputFormat::Webp, 95).unwrap();
        assert!(
            low.len() < high.len(),
            "q10={} q95={}",
            low.len(),
            high.len()
        );
    }

    #[test]
    fn webp_keeps_alpha() {
        let bytes = encode(&transparent_rgba(), OutputFormat::Webp, 80).unwrap();
        let back = image::load_from_memory(&bytes).unwrap();
        assert!(back.color().has_alpha());
        assert_eq!(back.to_rgba8().get_pixel(0, 0)[3], 0);
    }

    #[test]
    fn jpeg_flattens_transparency_to_white() {
        let bytes = encode(&transparent_rgba(), OutputFormat::Jpeg, 90).unwrap();
        let back = image::load_from_memory(&bytes).unwrap().to_rgb8();
        let p = back.get_pixel(4, 4);
        assert!(
            p.0.iter().all(|&c| c > 250),
            "투명 영역은 흰색이어야 함: {p:?}"
        );
    }

    #[test]
    fn png_is_lossless() {
        let src = sample_rgb();
        let bytes = encode(&src, OutputFormat::Png, 1).unwrap();
        let back = image::load_from_memory(&bytes).unwrap();
        assert_eq!(back.as_bytes(), src.as_bytes());
    }

    #[test]
    fn webp_rejects_oversized_dimensions() {
        let wide = DynamicImage::new_rgb8(WEBP_MAX_DIMENSION + 1, 1);
        assert!(matches!(
            encode(&wide, OutputFormat::Webp, 80),
            Err(PixelVaultError::InvalidOption(_))
        ));
    }
}
