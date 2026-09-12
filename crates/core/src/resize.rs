//! 2단계: 픽셀 → 더 작은 픽셀
//!
//! `fast_image_resize` 를 쓴다. `image::imageops::resize` 보다 빠른 이유:
//!
//! 1. SIMD — 한 명령어로 픽셀 여러 개를 동시에 계산
//! 2. 컨볼루션을 가로/세로 두 번의 1차원 패스로 분리
//! 3. 필터 계수를 미리 계산해 정수 연산으로 처리
//!
//! 자세한 건 docs/m1-core-pipeline.md.

use fast_image_resize::images::{TypedImage, TypedImageRef};
use fast_image_resize::pixels::{U8x3, U8x4};
use fast_image_resize::{
    FilterType, ImageBufferError, PixelTrait, ResizeAlg, ResizeOptions, Resizer,
};
use image::{DynamicImage, ImageBuffer, Pixel};

use crate::error::{PixelVaultError, Result};

/// 원본 비율을 유지하면서 `max_width × max_height` 상자 안에 들어가는 크기를 계산한다.
///
/// - 확대는 하지 않는다 (작은 이미지는 원래 크기 그대로).
/// - `None` 은 "그 방향으로는 제한 없음".
/// - 결과는 최소 1×1.
pub fn fit_within(
    width: u32,
    height: u32,
    max_width: Option<u32>,
    max_height: Option<u32>,
) -> (u32, u32) {
    let scale_w = max_width.map_or(1.0, |m| f64::from(m) / f64::from(width));
    let scale_h = max_height.map_or(1.0, |m| f64::from(m) / f64::from(height));
    let scale = scale_w.min(scale_h).min(1.0);

    if scale >= 1.0 {
        return (width, height);
    }
    let w = (f64::from(width) * scale).round().max(1.0) as u32;
    let h = (f64::from(height) * scale).round().max(1.0) as u32;
    (w, h)
}

/// 이미지를 `fit_within` 이 계산한 크기로 줄인다.
///
/// 소유권 흐름을 보자: `image` 를 값으로 받는다(move).
/// - 줄일 필요가 없으면 받은 걸 그대로 돌려준다 → 픽셀 복사 0회.
/// - 줄여야 하면 새 버퍼에 결과를 쓰고, 원본 `image` 는 이 함수가 끝날 때 drop(해제)된다.
///   호출한 쪽은 원본을 더 이상 쓸 수 없으므로 큰 원본 버퍼가 일찍 풀려서 메모리 피크가 낮아진다.
pub fn resize(
    image: DynamicImage,
    max_width: Option<u32>,
    max_height: Option<u32>,
) -> Result<DynamicImage> {
    let (w, h) = fit_within(image.width(), image.height(), max_width, max_height);
    if (w, h) == (image.width(), image.height()) {
        return Ok(image);
    }

    resized_copy(&image, max_width, max_height)
    // ← 여기서 원본 `image` 가 drop 된다
}

/// 원본을 **빌려서** 줄인 사본을 만든다. 원본은 호출자가 계속 쓸 수 있다.
///
/// `resize` 와의 차이: `resize(image)` 는 소유권을 가져가므로 "원본은 이제 필요 없음"을 뜻하고,
/// `resized_copy(&image)` 는 "원본도 계속 쓸 거니까 새로 하나 만들어 줘"를 뜻한다.
/// (예: BlurHash 는 인코딩할 원본과 별도로 32px 짜리 작은 사본만 필요하다)
pub fn resized_copy(
    image: &DynamicImage,
    max_width: Option<u32>,
    max_height: Option<u32>,
) -> Result<DynamicImage> {
    let (w, h) = fit_within(image.width(), image.height(), max_width, max_height);
    match image {
        DynamicImage::ImageRgb8(src) => {
            Ok(DynamicImage::ImageRgb8(resize_typed::<U8x3, _>(src, w, h)?))
        }
        DynamicImage::ImageRgba8(src) => Ok(DynamicImage::ImageRgba8(resize_typed::<U8x4, _>(
            src, w, h,
        )?)),
        // decode 단계에서 항상 RGB8/RGBA8 로 정규화하므로, 다른 형식이 오면 호출 순서가 잘못된 것
        other => Err(PixelVaultError::InvalidOption(format!(
            "resize expects RGB8/RGBA8, got {:?}",
            other.color()
        ))),
    }
}

/// 픽셀 타입(`P`)을 컴파일 타임에 고정해서 리사이즈한다.
///
/// `fast_image_resize` 에 `DynamicImage` 를 그대로 넘기면(런타임 분기) 16비트/float 픽셀용 코드와
/// 512 KB 짜리 16비트 알파 역수 테이블까지 전부 wasm 에 링크된다. 우리는 RGB8/RGBA8 만 쓰므로
/// 제네릭으로 두 타입만 인스턴스화(monomorphization)하면 그 코드들이 링크 단계에서 떨어져 나간다.
fn resize_typed<P, Px>(
    src: &ImageBuffer<Px, Vec<u8>>,
    width: u32,
    height: u32,
) -> Result<ImageBuffer<Px, Vec<u8>>>
where
    P: PixelTrait,
    Px: Pixel<Subpixel = u8>,
{
    // 원본 픽셀 버퍼(&[u8])를 복사 없이 "P 타입 픽셀의 배열"로 다시 해석한 뷰
    let src_view = TypedImageRef::<P>::from_buffer(src.width(), src.height(), src.as_raw())
        .map_err(buffer_error)?;

    let mut dst = ImageBuffer::<Px, Vec<u8>>::new(width, height);
    let mut dst_view =
        TypedImage::<P>::from_buffer(width, height, &mut dst).map_err(buffer_error)?;

    // Lanczos3: 축소 시 선명도가 좋은 표준 필터. (Bilinear 보다 느리지만 품질이 확실히 좋다)
    // RGBA 는 기본적으로 알파를 곱해서(premultiply) 계산하므로 투명 경계가 검게 번지지 않는다.
    let options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3));
    Resizer::new().resize_typed(&src_view, &mut dst_view, &options)?;

    Ok(dst)
}

fn buffer_error(e: ImageBufferError) -> PixelVaultError {
    PixelVaultError::InvalidOption(format!("pixel buffer mismatch: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ColorType, GenericImageView, Rgba, RgbaImage};

    #[test]
    fn fit_keeps_aspect_ratio() {
        assert_eq!(fit_within(4000, 3000, Some(1920), None), (1920, 1440));
        assert_eq!(fit_within(3000, 4000, Some(1920), Some(1920)), (1440, 1920));
        assert_eq!(fit_within(4000, 3000, None, Some(300)), (400, 300));
    }

    #[test]
    fn fit_never_upscales() {
        assert_eq!(fit_within(800, 600, Some(1920), Some(1080)), (800, 600));
        assert_eq!(fit_within(800, 600, None, None), (800, 600));
    }

    #[test]
    fn fit_never_returns_zero() {
        assert_eq!(fit_within(10000, 10, Some(100), None), (100, 1));
    }

    #[test]
    fn resize_changes_dimensions_and_keeps_format() {
        let img = DynamicImage::new_rgb8(640, 480);
        let out = resize(img, Some(320), None).unwrap();
        assert_eq!(out.dimensions(), (320, 240));
        assert_eq!(out.color(), ColorType::Rgb8);
    }

    #[test]
    fn resize_is_noop_when_already_small() {
        let img = DynamicImage::new_rgba8(100, 50);
        let ptr_before = img.as_bytes().as_ptr();
        let out = resize(img, Some(1920), None).unwrap();
        // 같은 버퍼 주소 → 복사 없이 소유권만 돌아왔다는 증거
        assert_eq!(out.as_bytes().as_ptr(), ptr_before);
    }

    #[test]
    fn resize_premultiplies_alpha() {
        // 왼쪽 절반: 불투명 흰색, 오른쪽 절반: 완전 투명 "검정"(RGB=0).
        // 알파를 무시하고 섞으면 경계가 회색으로 어두워진다. premultiply 하면 흰색이 유지된다.
        let src = RgbaImage::from_fn(64, 64, |x, _| {
            if x < 32 {
                Rgba([255, 255, 255, 255])
            } else {
                Rgba([0, 0, 0, 0])
            }
        });
        let out = resize(DynamicImage::ImageRgba8(src), Some(16), None).unwrap();
        let rgba = out.to_rgba8();
        for y in 0..16 {
            let p = rgba.get_pixel(7, y); // 경계 바로 왼쪽
            assert!(p[3] > 0);
            assert!(p[0] > 240, "경계 픽셀이 어두워짐: {p:?}");
        }
    }
}
