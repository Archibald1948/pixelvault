//! 벤치마크용 "사진 같은" 큰 JPEG 를 만든다 (git 에는 올리지 않음, bench/fixtures/ 는 gitignore).
//!
//!     cargo run --release -p pixelvault-core --example gen_bench_image
//!
//! 실제 사진처럼 부드러운 그라데이션 + 중간 주파수 무늬 + 픽셀 단위 노이즈(센서 노이즈 흉내)를 섞는다.
//! 노이즈가 없으면 JPEG/WebP 가 비현실적으로 잘 압축되어 벤치마크가 의미 없어진다.

use std::fs;
use std::path::Path;

use image::RgbImage;
use image::codecs::jpeg::JpegEncoder;

fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bench/fixtures");
    fs::create_dir_all(&dir).unwrap();

    // (파일명, 가로, 세로, JPEG 품질)
    for (name, w, h, q) in [
        ("photo-12mp.jpg", 4032, 3024, 92), // 아이폰 기본 해상도
        ("photo-24mp.jpg", 6000, 4000, 90), // 미러리스급, 10MB 전후
    ] {
        let img = photo_like(w, h);
        let mut out = Vec::new();
        JpegEncoder::new_with_quality(&mut out, q)
            .encode_image(&img)
            .unwrap();
        fs::write(dir.join(name), &out).unwrap();
        println!("{name}: {w}x{h}, {:.1} MB", out.len() as f64 / 1e6);
    }
}

fn photo_like(w: u32, h: u32) -> RgbImage {
    RgbImage::from_fn(w, h, |x, y| {
        let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
        // 저주파: 하늘 → 땅 그라데이션
        let base = [
            80.0 + 120.0 * fy,
            140.0 + 60.0 * (1.0 - fy),
            200.0 - 120.0 * fy,
        ];
        // 중간 주파수: 나뭇잎/질감 흉내
        let texture =
            25.0 * ((fx * 90.0).sin() * (fy * 70.0).cos() + (fx * 13.0 + fy * 17.0).sin());
        // 고주파: 결정적(deterministic) 해시 노이즈
        let n = hash(x, y);
        let mut px = [0u8; 3];
        for c in 0..3 {
            let noise = ((n >> (c * 8)) & 0xFF) as f32 / 255.0 * 24.0 - 12.0;
            px[c] = (base[c] + texture + noise).clamp(0.0, 255.0) as u8;
        }
        image::Rgb(px)
    })
}

fn hash(x: u32, y: u32) -> u32 {
    let mut h = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ (h >> 12)
}
