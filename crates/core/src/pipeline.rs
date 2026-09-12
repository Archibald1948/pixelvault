//! 오케스트레이터: decode → (orient) → resize → encode 를 순서대로 조립한다.
//!
//! 이 파일을 읽으면 `DynamicImage` 의 소유권이 파이프라인을 따라 어떻게 흘러가는지 보인다.
//!
//! ```text
//! input: &[u8] ──decode──▶ Decoded { image, icc, exif }
//!  (JS 가 준 바이트를         │ 구조 분해로 필드별 소유권을 꺼냄
//!   빌려서 읽기만 함)          ▼
//!                 image ──resize(move)──▶ image ──orient(move)──▶ image ──encode(&borrow)──▶ Vec<u8>
//!                        (원본 픽셀은 여기서 해제)                     (크기·BlurHash 에 계속 사용)
//! ```

use image::ImageFormat;

use crate::decode::{Decoded, decode};
use crate::encode::{DEFAULT_QUALITY, JPEG_MAX_EXIF, Metadata, OutputFormat, encode_with_metadata};
use crate::error::{PixelVaultError, Result};
use crate::exif;
use crate::resize::resize;

/// 파이프라인 옵션. JS 쪽 `ProcessOptions` 인터페이스와 1:1 대응한다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessOptions {
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    pub format: OutputFormat,
    /// 1–100
    pub quality: u8,
    /// EXIF(촬영 정보·GPS)를 결과에서 뺄지. 기본 true.
    pub strip_exif: bool,
    /// BlurHash 문자열을 같이 만들지. 기본 false.
    pub blurhash: bool,
}

/// `ProcessOptions { format: OutputFormat::Jpeg, ..Default::default() }` 처럼
/// 필요한 필드만 바꿔 쓸 수 있게 기본값을 정의해 둔다.
impl Default for ProcessOptions {
    fn default() -> Self {
        Self {
            max_width: None,
            max_height: None,
            format: OutputFormat::Webp,
            quality: DEFAULT_QUALITY,
            strip_exif: true,
            blurhash: false,
        }
    }
}

/// 파이프라인 결과
#[derive(Debug, Clone)]
pub struct ProcessOutput {
    pub bytes: Vec<u8>,
    /// 방향 보정 후(= 화면에 보이는) 크기
    pub width: u32,
    pub height: u32,
    pub format: OutputFormat,
    pub source_format: ImageFormat,
    pub original_bytes: usize,
    pub output_bytes: usize,
    pub blurhash: Option<String>,
    /// 원본 EXIF 의 Orientation (1–8). 1 이 아니었다면 픽셀에 회전을 반영했다는 뜻.
    pub source_orientation: u8,
    /// 원본에 GPS 정보가 있었는지
    pub had_gps: bool,
    /// 결과 파일에 EXIF 가 남아 있는지 (stripExif=false 이고 원본에 EXIF 가 있었을 때만 true)
    pub exif_kept: bool,
}

/// 이미지 한 장을 처리한다.
///
/// 에러가 어느 단계에서 나든 `?` 가 즉시 `Err` 를 호출자에게 돌려준다.
/// (JS 의 try/catch 대신, 실패 가능성이 함수 시그니처 `Result<…>` 에 드러나 있다)
pub fn process(input: &[u8], options: &ProcessOptions) -> Result<ProcessOutput> {
    validate(options)?;

    // 구조 분해(destructuring): 구조체를 필드별로 쪼개서 각각의 소유권을 따로 가져온다.
    let Decoded {
        image,
        format: source_format,
        icc,
        exif,
    } = decode(input)?;

    // as_deref: Option<Vec<u8>> → Option<&[u8]> (소유권은 그대로 두고 안을 빌려 봄)
    let summary = exif.as_deref().map(exif::summarize).unwrap_or_default();

    // ── 방향 보정 + 리사이즈 ─────────────────────────────────────────────
    // 사용자의 maxWidth/maxHeight 는 "화면에 보이는" 방향 기준이다.
    // 90° 회전이 필요한 사진은 저장된 픽셀의 가로가 화면의 세로이므로 제한을 뒤바꿔서 리사이즈한다.
    // 그리고 회전은 리사이즈 **후에** 한다: 12MP 원본(36MB)을 돌리는 것보다
    // 1920px 결과(8MB)를 돌리는 게 훨씬 싸다. 90° 회전과 리사이즈는 순서를 바꿔도 결과가 같다.
    let (max_w, max_h) = if exif::swaps_dimensions(summary.orientation) {
        (options.max_height, options.max_width)
    } else {
        (options.max_width, options.max_height)
    };
    let image = resize(image, max_w, max_h)?;
    let image = exif::apply_orientation(image, summary.orientation);

    let blurhash = options
        .blurhash
        .then(|| crate::blurhash::encode(&image))
        .transpose()?; // Option<Result<T>> → Result<Option<T>>, 그리고 ? 로 에러 전파

    // ── 메타데이터 ──────────────────────────────────────────────────────
    // EXIF: 기본은 버린다(픽셀만 다시 인코딩하므로 아무것도 안 하면 자동으로 사라진다).
    // 유지하는 경우엔 Orientation 을 1 로 되돌린다. 픽셀에 이미 회전을 반영했으니까.
    let kept_exif = if options.strip_exif {
        None
    } else {
        exif.map(|mut raw| {
            exif::reset_orientation(&mut raw);
            raw
        })
    };
    let kept_exif =
        kept_exif.filter(|e| options.format != OutputFormat::Jpeg || e.len() <= JPEG_MAX_EXIF);

    let meta = Metadata {
        icc: icc.as_deref(), // 색 프로파일은 개인정보가 아니므로 항상 유지
        exif: kept_exif.as_deref(),
    };

    // 인코딩은 빌려서만 한다. 그래서 아래에서 image.width() 를 계속 쓸 수 있다.
    let bytes = encode_with_metadata(&image, options.format, options.quality, meta)?;

    Ok(ProcessOutput {
        width: image.width(),
        height: image.height(),
        format: options.format,
        source_format,
        original_bytes: input.len(),
        output_bytes: bytes.len(),
        bytes, // 마지막에 move. 위의 bytes.len() 보다 뒤에 있어야 한다.
        blurhash,
        source_orientation: summary.orientation,
        had_gps: summary.has_gps,
        exif_kept: kept_exif.is_some(),
    })
}

fn validate(options: &ProcessOptions) -> Result<()> {
    if options.max_width == Some(0) || options.max_height == Some(0) {
        return Err(PixelVaultError::InvalidOption(
            "maxWidth/maxHeight must be greater than 0".into(),
        ));
    }
    if !(1..=100).contains(&options.quality) {
        return Err(PixelVaultError::InvalidOption(format!(
            "quality must be 1-100, got {}",
            options.quality
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GenericImageView, Rgba};
    use std::io::Cursor;

    const JPEG: &[u8] = include_bytes!("../tests/fixtures/landscape.jpg");
    const PNG_ALPHA: &[u8] = include_bytes!("../tests/fixtures/transparent.png");
    /// 저장된 픽셀은 400×300(가로), EXIF Orientation=6 + GPS. 올바르게 보면 300×400 세로 사진.
    const IPHONE: &[u8] = include_bytes!("../tests/fixtures/iphone-portrait.jpg");
    /// 가짜 ICC 프로파일("pixelvault-test-icc")이 들어 있는 JPEG
    const WITH_ICC: &[u8] = include_bytes!("../tests/fixtures/with-icc.jpg");

    const ALL_FORMATS: [OutputFormat; 3] =
        [OutputFormat::Webp, OutputFormat::Jpeg, OutputFormat::Png];

    /// 결과 파일을 kamadak-exif 로 다시 열어서 EXIF 를 읽는다. 없으면 None.
    fn read_output_exif(bytes: &[u8]) -> Option<::exif::Exif> {
        ::exif::Reader::new()
            .read_from_container(&mut Cursor::new(bytes))
            .ok()
    }

    fn is_close(p: Rgba<u8>, rgb: [u8; 3]) -> bool {
        p.0[..3].iter().zip(rgb).all(|(&a, b)| a.abs_diff(b) < 40)
    }

    #[test]
    fn jpeg_to_resized_webp() {
        let out = process(
            JPEG,
            &ProcessOptions {
                max_width: Some(320),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!((out.width, out.height), (320, 240));
        assert_eq!(out.format, OutputFormat::Webp);
        assert_eq!(out.source_format, ImageFormat::Jpeg);
        assert_eq!(out.original_bytes, JPEG.len());
        assert_eq!(out.output_bytes, out.bytes.len());
        assert_eq!(image::guess_format(&out.bytes).unwrap(), ImageFormat::WebP);
        assert!(out.blurhash.is_none());
    }

    #[test]
    fn png_with_alpha_to_jpeg() {
        let out = process(
            PNG_ALPHA,
            &ProcessOptions {
                format: OutputFormat::Jpeg,
                quality: 90,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(image::guess_format(&out.bytes).unwrap(), ImageFormat::Jpeg);
    }

    #[test]
    fn rejects_zero_max_width() {
        let err = process(
            JPEG,
            &ProcessOptions {
                max_width: Some(0),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(matches!(err, PixelVaultError::InvalidOption(_)));
    }

    // ── M2: EXIF ─────────────────────────────────────────────────────────

    #[test]
    fn iphone_portrait_is_not_lying_down() {
        // Definition of Done: "아이폰 세로 사진(Orientation=6)이 눕지 않는다"
        for format in ALL_FORMATS {
            let out = process(
                IPHONE,
                &ProcessOptions {
                    format,
                    quality: 90,
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(
                (out.width, out.height),
                (300, 400),
                "{format}: 세로 사진이어야 함"
            );
            assert_eq!(out.source_orientation, 6);

            // 올바른 방향이면 해는 왼쪽 위, 땅(초록)은 아래에 있다
            let img = image::load_from_memory(&out.bytes).unwrap();
            assert!(
                is_close(img.get_pixel(60, 80), [250, 210, 40]),
                "{format}: 해가 왼쪽 위에 없음"
            );
            assert!(
                is_close(img.get_pixel(150, 380), [40, 110, 50]),
                "{format}: 땅이 아래에 없음"
            );
        }
    }

    #[test]
    fn gps_is_removed_by_default() {
        // Definition of Done: "출력물에 EXIF GPS 태그가 남아있지 않다"
        for format in ALL_FORMATS {
            let out = process(
                IPHONE,
                &ProcessOptions {
                    format,
                    ..Default::default()
                },
            )
            .unwrap();
            assert!(out.had_gps, "원본에는 GPS 가 있었다");
            assert!(!out.exif_kept);
            assert!(
                read_output_exif(&out.bytes).is_none(),
                "{format}: 결과에 EXIF 가 남아 있음"
            );
        }
    }

    #[test]
    fn keeping_exif_resets_orientation_to_avoid_double_rotation() {
        for format in ALL_FORMATS {
            let out = process(
                IPHONE,
                &ProcessOptions {
                    format,
                    strip_exif: false,
                    ..Default::default()
                },
            )
            .unwrap();
            assert!(out.exif_kept, "{format}");
            let exif =
                read_output_exif(&out.bytes).unwrap_or_else(|| panic!("{format}: EXIF 없음"));
            let orientation = exif
                .get_field(::exif::Tag::Orientation, ::exif::In::PRIMARY)
                .and_then(|f| f.value.get_uint(0));
            assert_eq!(
                orientation,
                Some(1),
                "{format}: 픽셀은 이미 돌렸으니 태그는 1 이어야 함"
            );
            // 사용자가 유지를 선택했으니 GPS 도 남아 있다
            assert!(
                exif.get_field(::exif::Tag::GPSLatitude, ::exif::In::PRIMARY)
                    .is_some()
            );
        }
    }

    #[test]
    fn max_width_applies_to_displayed_orientation() {
        let out = process(
            IPHONE,
            &ProcessOptions {
                max_width: Some(150),
                ..Default::default()
            },
        )
        .unwrap();
        // 화면 기준 가로 150 → 세로 200. (저장 기준으로 계산했다면 150×113 이 되었을 것)
        assert_eq!((out.width, out.height), (150, 200));
    }

    #[test]
    fn icc_profile_is_preserved_in_every_format() {
        for format in ALL_FORMATS {
            let out = process(
                WITH_ICC,
                &ProcessOptions {
                    format,
                    ..Default::default()
                },
            )
            .unwrap();
            let decoded = decode(&out.bytes).unwrap();
            assert_eq!(
                decoded.icc.as_deref(),
                Some(&b"pixelvault-test-icc"[..]),
                "{format}: ICC 가 사라짐"
            );
        }
    }

    // ── M2: BlurHash ─────────────────────────────────────────────────────

    #[test]
    fn blurhash_is_generated_on_request() {
        let out = process(
            IPHONE,
            &ProcessOptions {
                blurhash: true,
                ..Default::default()
            },
        )
        .unwrap();
        let hash = out.blurhash.expect("blurhash requested");
        // 세로 사진 → 3×4 성분 → 4 + 2×11 = 28자
        assert_eq!(hash.len(), 28);
    }
}
