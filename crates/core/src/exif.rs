//! EXIF 메타데이터: 요약(Orientation, GPS 유무), 방향 보정, 방향 태그 초기화.
//!
//! EXIF 는 카메라가 사진에 붙이는 메타데이터 묶음이다. 촬영 시각, 카메라 모델, **GPS 좌표**,
//! 그리고 **Orientation(방향)** 이 들어 있다.
//!
//! 아이폰은 세로로 찍어도 센서 기준(가로)으로 픽셀을 저장하고, "보여줄 때 90° 돌려라"(Orientation=6)
//! 라는 태그만 붙인다. 픽셀만 다시 인코딩하면서 EXIF 를 지우면 이 정보가 사라져 **사진이 눕는다.**
//! 그래서 EXIF 를 지우기 전에 회전을 픽셀에 먼저 반영해야 한다.

use exif::{Context, In, Reader, Tag};
use image::DynamicImage;
use image::metadata::Orientation;

/// 파이프라인이 알아야 하는 EXIF 정보 요약
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExifSummary {
    /// EXIF Orientation 값 (1–8, 없거나 이상하면 1)
    pub orientation: u8,
    /// GPS 태그가 하나라도 있었는지 (UI 에서 "위치 정보 제거됨"을 보여주기 위해)
    pub has_gps: bool,
}

impl Default for ExifSummary {
    fn default() -> Self {
        Self {
            orientation: 1,
            has_gps: false,
        }
    }
}

/// EXIF 원본 바이트(TIFF 구조)를 파싱해서 필요한 정보만 뽑는다.
///
/// 깨진 EXIF 는 흔하다(편집 앱들이 제멋대로 쓴다). 그래서 파싱 실패는 에러가 아니라
/// "정보 없음"(기본값)으로 처리한다. 메타데이터 때문에 이미지 변환 전체가 실패하면 안 되니까.
pub fn summarize(raw_exif: &[u8]) -> ExifSummary {
    // kamadak-exif 의 read_raw 는 Vec<u8> 을 소유권째 요구한다 → 여기서 한 번 복사.
    let Ok(exif) = Reader::new().read_raw(raw_exif.to_vec()) else {
        return ExifSummary::default();
    };

    let orientation = exif
        .get_field(Tag::Orientation, In::PRIMARY)
        .and_then(|field| field.value.get_uint(0))
        .and_then(|v| u8::try_from(v).ok())
        .filter(|v| (1..=8).contains(v))
        .unwrap_or(1);

    let has_gps = exif.fields().any(|f| f.tag.context() == Context::Gps);

    ExifSummary {
        orientation,
        has_gps,
    }
}

/// Orientation 5~8 은 90°/270° 회전이 들어가서 가로·세로가 뒤바뀐다.
pub fn swaps_dimensions(orientation: u8) -> bool {
    matches!(orientation, 5..=8)
}

/// EXIF Orientation 에 맞게 픽셀을 실제로 돌리고/뒤집는다.
///
/// ```text
///  1: 그대로     2: 좌우반전    3: 180°      4: 상하반전
///  5: 90°+좌우   6: 90° (CW)    7: 270°+좌우  8: 270° (CW)
/// ```
/// 아이폰 세로 사진은 6, 거꾸로 든 폰은 8, 뒤집힌 폰은 3 이 흔하다.
///
/// `image` 를 값으로 받아 값으로 돌려준다. 1(그대로)이면 복사 없이 그대로 반환된다.
pub fn apply_orientation(mut image: DynamicImage, orientation: u8) -> DynamicImage {
    if let Some(o) = Orientation::from_exif(orientation) {
        image.apply_orientation(o);
    }
    image
}

/// EXIF 를 유지하기로 했을 때: 픽셀에 회전을 이미 반영했으므로 태그를 1(그대로)로 되돌린다.
/// 안 그러면 뷰어가 한 번 더 돌려서 **이중 회전**된다.
///
/// `&mut [u8]` — 슬라이스를 가변으로 빌려서 제자리(in-place)에서 2바이트만 고친다. 새 할당 없음.
pub fn reset_orientation(raw_exif: &mut [u8]) {
    let _ = Orientation::remove_from_exif_chunk(raw_exif);
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GenericImageView, Rgb, RgbImage};

    const IPHONE_EXIF: &[u8] = include_bytes!("../tests/fixtures/exif-orientation6-gps.bin");

    #[test]
    fn reads_orientation_and_gps() {
        let s = summarize(IPHONE_EXIF);
        assert_eq!(s.orientation, 6);
        assert!(s.has_gps);
    }

    #[test]
    fn garbage_exif_is_not_an_error() {
        assert_eq!(summarize(b"not exif at all"), ExifSummary::default());
        assert_eq!(summarize(&[]), ExifSummary::default());
    }

    #[test]
    fn reset_orientation_rewrites_tag_in_place() {
        let mut exif = IPHONE_EXIF.to_vec();
        reset_orientation(&mut exif);
        let s = summarize(&exif);
        assert_eq!(s.orientation, 1);
        assert!(s.has_gps, "다른 태그는 건드리지 않는다");
        assert_eq!(exif.len(), IPHONE_EXIF.len());
    }

    #[test]
    fn orientation_6_rotates_clockwise() {
        // 2×1 이미지: [빨강, 파랑]. 시계방향 90° → 1×2: 위 빨강, 아래 파랑
        let mut img = RgbImage::new(2, 1);
        img.put_pixel(0, 0, Rgb([255, 0, 0]));
        img.put_pixel(1, 0, Rgb([0, 0, 255]));
        let out = apply_orientation(DynamicImage::ImageRgb8(img), 6);
        assert_eq!(out.dimensions(), (1, 2));
        assert_eq!(out.get_pixel(0, 0).0[..3], [255, 0, 0]);
        assert_eq!(out.get_pixel(0, 1).0[..3], [0, 0, 255]);
    }

    #[test]
    fn swaps_only_for_90_degree_orientations() {
        let swapping: Vec<u8> = (1..=8).filter(|&o| swaps_dimensions(o)).collect();
        assert_eq!(swapping, vec![5, 6, 7, 8]);
    }
}
