//! BlurHash: 이미지를 20~30자 문자열로 요약한 "흐릿한 미리보기".
//!
//! 이미지를 몇 개의 코사인 성분(DCT 와 비슷)으로 근사해서 base83 문자열로 인코딩한다.
//! 서버 DB 에 이 문자열만 저장해 두면, 진짜 이미지가 로드되기 전에 색감이 맞는 흐린 플레이스홀더를
//! 즉시 그릴 수 있다. (예: `LEHV6nWB2yk8pyo0adR*.7kCMdnj`)

use image::DynamicImage;

use crate::error::{PixelVaultError, Result};
use crate::resize;

/// BlurHash 계산 전에 이미지를 이 크기 안으로 줄인다.
///
/// 인코딩 비용은 `가로 × 세로 × 성분 수` 에 비례한다. 1920×1440 을 그대로 넣으면 수백 ms 가 걸리지만,
/// 결과는 어차피 4×3 개의 성분뿐이라 32px 로 줄여도 문자열은 사실상 같다.
const SAMPLE_SIZE: u32 = 32;

/// 이미지의 BlurHash 문자열을 만든다.
pub fn encode(image: &DynamicImage) -> Result<String> {
    // 원본은 빌리기만 하고(&) 작은 사본을 새로 만든다. 원본은 인코딩 단계에서 계속 써야 하므로.
    // (image.clone() 후 resize 하면 1920px 원본 전체를 한 번 더 복사하게 된다)
    let small = resize::resized_copy(image, Some(SAMPLE_SIZE), Some(SAMPLE_SIZE))?;
    let rgba = small.to_rgba8();
    let (cx, cy) = components_for(rgba.width(), rgba.height());

    // `::blurhash` = 외부 크레이트. 이 모듈 이름도 blurhash 라서 앞에 :: 를 붙여 구분한다.
    ::blurhash::encode(cx, cy, rgba.width(), rgba.height(), rgba.as_raw())
        .map_err(|e| PixelVaultError::InvalidOption(format!("blurhash: {e}")))
}

/// 가로로 긴 이미지는 가로 성분을 더 많이 (4×3), 세로로 긴 이미지는 반대로 (3×4).
fn components_for(width: u32, height: u32) -> (u32, u32) {
    if width >= height { (4, 3) } else { (3, 4) }
}

/// BlurHash 문자열을 `width × height` RGBA 픽셀로 되돌린다 (플레이스홀더 그리기용).
pub fn decode(hash: &str, width: u32, height: u32) -> Result<Vec<u8>> {
    ::blurhash::decode(hash, width, height, 1.0)
        .map_err(|e| PixelVaultError::InvalidOption(format!("blurhash: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    #[test]
    fn produces_valid_hash_for_landscape() {
        let img = DynamicImage::ImageRgb8(RgbImage::from_pixel(400, 300, Rgb([200, 80, 40])));
        let hash = encode(&img).unwrap();
        // 길이 = 4(헤더) + 2 × 성분 수. 4×3 = 12 성분 → 4 + 2×11 = 28
        assert_eq!(hash.len(), 28, "{hash}");
    }

    #[test]
    fn round_trip_keeps_average_color() {
        let img = DynamicImage::ImageRgb8(RgbImage::from_pixel(64, 64, Rgb([200, 80, 40])));
        let hash = encode(&img).unwrap();
        let px = decode(&hash, 32, 32).unwrap();
        assert_eq!(px.len(), 32 * 32 * 4);

        // 개별 픽셀은 ±15 정도 흔들린다: BlurHash 는 AC 성분을 19단계로 양자화하는데
        // 가장 작은 단계도 0 이 아니라서, 단색 이미지에도 약한 무늬가 생긴다(알고리즘 특성).
        // 대신 평균 색(DC 성분)은 정확히 보존된다.
        let n = (px.len() / 4) as i32;
        let avg = |c: usize| {
            px.as_chunks::<4>()
                .0
                .iter()
                .map(|p| i32::from(p[c]))
                .sum::<i32>()
                / n
        };
        let (r, g, b) = (avg(0), avg(1), avg(2));
        assert!(
            (r - 200).abs() <= 2 && (g - 80).abs() <= 2 && (b - 40).abs() <= 2,
            "{r},{g},{b}"
        );
    }

    #[test]
    fn portrait_uses_more_vertical_components() {
        assert_eq!(components_for(300, 400), (3, 4));
        assert_eq!(components_for(400, 300), (4, 3));
    }

    #[test]
    fn rejects_bad_hash() {
        assert!(decode("??", 8, 8).is_err());
    }
}
