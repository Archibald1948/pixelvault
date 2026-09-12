//! 테스트용 샘플 이미지를 생성한다. 결과물은 `tests/fixtures/` 에 커밋되어 있고,
//! 이 예제는 "어떻게 만들어졌는지"를 재현할 수 있게 남겨 둔 것이다.
//!
//!     cargo run -p pixelvault-core --example gen_fixtures

use std::fs;
use std::path::Path;

use exif::experimental::Writer;
use exif::{Field, In, Rational, Tag, Value};
use image::codecs::jpeg::JpegEncoder;
use image::{
    DynamicImage, ImageBuffer, ImageEncoder, ImageFormat, Luma, Rgb, RgbImage, Rgba, RgbaImage,
};

fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    fs::create_dir_all(&dir).unwrap();

    // 640×480 풍경: 위는 하늘(파랑 그라데이션), 아래는 땅(초록), 왼쪽 위에 해(노랑).
    // 상하좌우가 비대칭이라 회전(Orientation) 테스트에도 쓸 수 있다.
    let big = landscape(640, 480);
    let mut jpeg = Vec::new();
    JpegEncoder::new_with_quality(&mut jpeg, 90)
        .encode_image(&big)
        .unwrap();
    fs::write(dir.join("landscape.jpg"), &jpeg).unwrap();

    // 256×256 RGBA: 투명 배경 위 반투명 원
    let transparent = RgbaImage::from_fn(256, 256, |x, y| {
        let (dx, dy) = (x as f32 - 128.0, y as f32 - 128.0);
        if dx * dx + dy * dy < 100.0 * 100.0 {
            Rgba([230, 80, 40, 200])
        } else {
            Rgba([0, 0, 0, 0])
        }
    });
    transparent
        .save_with_format(dir.join("transparent.png"), ImageFormat::Png)
        .unwrap();

    // 64×64 16비트 흑백 PNG: 정규화(→ RGB8) 테스트용
    let gray16: ImageBuffer<Luma<u16>, Vec<u16>> =
        ImageBuffer::from_fn(64, 64, |x, y| Luma([((x + y) * 512) as u16]));
    DynamicImage::ImageLuma16(gray16)
        .save_with_format(dir.join("gray16.png"), ImageFormat::Png)
        .unwrap();

    // 320×240 손실 WebP: WebP 입력 디코드 테스트용
    let small = landscape(320, 240);
    let webp = webp::Encoder::from_rgb(small.as_raw(), 320, 240).encode(75.0);
    fs::write(dir.join("photo.webp"), &*webp).unwrap();

    // ── M2: EXIF ────────────────────────────────────────────────────────
    // "아이폰 세로 사진" 흉내: 화면에 보여야 할 모습은 300×400 세로 풍경(해는 왼쪽 위).
    // 센서는 가로로 저장하므로 픽셀은 반시계 90° 돌린 400×300 으로 저장하고,
    // EXIF Orientation=6("보여줄 때 시계방향 90° 돌려라")을 붙인다.
    let upright = DynamicImage::ImageRgb8(landscape(300, 400));
    let stored = upright.rotate270().to_rgb8();
    let exif = iphone_exif();
    fs::write(dir.join("exif-orientation6-gps.bin"), &exif).unwrap();

    let mut jpeg = Vec::new();
    let mut enc = JpegEncoder::new_with_quality(&mut jpeg, 92);
    enc.set_exif_metadata(exif).unwrap();
    enc.write_image(
        stored.as_raw(),
        stored.width(),
        stored.height(),
        image::ExtendedColorType::Rgb8,
    )
    .unwrap();
    fs::write(dir.join("iphone-portrait.jpg"), &jpeg).unwrap();

    // ICC 프로파일 보존 테스트용 (진짜 프로파일일 필요는 없다 — 바이트가 그대로 옮겨지는지만 본다)
    let mut jpeg = Vec::new();
    let mut enc = JpegEncoder::new_with_quality(&mut jpeg, 80);
    enc.set_icc_profile(b"pixelvault-test-icc".to_vec())
        .unwrap();
    let small = landscape(64, 48);
    enc.write_image(small.as_raw(), 64, 48, image::ExtendedColorType::Rgb8)
        .unwrap();
    fs::write(dir.join("with-icc.jpg"), &jpeg).unwrap();

    println!("fixtures written to {}", dir.display());
}

/// 아이폰이 붙이는 것과 비슷한 EXIF: 제조사/모델, Orientation=6, 서울 좌표 GPS.
fn iphone_exif() -> Vec<u8> {
    let ascii = |s: &str| Value::Ascii(vec![s.as_bytes().to_vec()]);
    let dms = |d: u32, m: u32, s_hundredths: u32| {
        Value::Rational(vec![
            Rational::from((d, 1)),
            Rational::from((m, 1)),
            Rational::from((s_hundredths, 100)),
        ])
    };
    let field = |tag, value| Field {
        tag,
        ifd_num: In::PRIMARY,
        value,
    };
    let fields = [
        field(Tag::Make, ascii("Apple")),
        field(Tag::Model, ascii("iPhone 15 Pro")),
        field(Tag::Orientation, Value::Short(vec![6])),
        field(Tag::DateTimeOriginal, ascii("2026:09:01 12:34:56")),
        field(Tag::GPSLatitudeRef, ascii("N")),
        field(Tag::GPSLatitude, dms(37, 33, 3624)),
        field(Tag::GPSLongitudeRef, ascii("E")),
        field(Tag::GPSLongitude, dms(126, 58, 4180)),
    ];

    let mut writer = Writer::new();
    for f in &fields {
        writer.push_field(f);
    }
    let mut buf = std::io::Cursor::new(Vec::new());
    writer.write(&mut buf, true).unwrap(); // true = 리틀엔디언("II"), 아이폰과 같다
    buf.into_inner()
}

fn landscape(w: u32, h: u32) -> RgbImage {
    let horizon = h * 2 / 3;
    let (sun_x, sun_y, sun_r) = (w as f32 * 0.2, h as f32 * 0.2, h as f32 * 0.1);
    RgbImage::from_fn(w, h, |x, y| {
        let (dx, dy) = (x as f32 - sun_x, y as f32 - sun_y);
        if dx * dx + dy * dy < sun_r * sun_r {
            Rgb([250, 210, 40])
        } else if y < horizon {
            let t = y as f32 / horizon as f32;
            Rgb([(60.0 + 100.0 * t) as u8, (120.0 + 80.0 * t) as u8, 235])
        } else {
            let t = (y - horizon) as f32 / (h - horizon) as f32;
            Rgb([40, (150.0 - 60.0 * t) as u8, 50])
        }
    })
}
