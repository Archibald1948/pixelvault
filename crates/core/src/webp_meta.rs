//! WebP 파일에 ICC 색 프로파일 / EXIF 청크를 붙인다.
//!
//! libwebp 의 간단 인코딩 API 는 메타데이터를 넣어 주지 않는다. 대신 WebP 파일 구조가 단순해서
//! 바이트를 직접 조립할 수 있다. (바이트 슬라이스를 다루는 좋은 연습 문제)
//!
//! ```text
//! WebP = RIFF 컨테이너
//! ┌──────┬────────────┬──────┐
//! │"RIFF"│ size (u32LE)│"WEBP"│  + 청크들…
//! └──────┴────────────┴──────┘
//! 청크 = │ FourCC(4B) │ size(u32LE) │ payload(size B) │ (size 가 홀수면 패딩 1B) │
//!
//! 단순 형식:   RIFF WEBP [VP8 ]                       ← 메타데이터 넣을 자리가 없음
//! 확장 형식:   RIFF WEBP [VP8X] [ICCP] [ALPH] [VP8 ] [EXIF]
//!                         ^^^^ 플래그로 "ICC 있음/알파 있음/EXIF 있음"을 알리고 캔버스 크기를 적는 헤더
//! ```

use crate::error::{PixelVaultError, Result};

const FLAG_ICC: u8 = 0x20;
const FLAG_ALPHA: u8 = 0x10;
const FLAG_EXIF: u8 = 0x08;

/// 파싱된 청크 하나. `payload` 는 원본 버퍼를 **빌린** 슬라이스라 복사가 없다.
///
/// `'a` 는 라이프타임 표시: "이 Chunk 는 원본 바이트(`'a`)보다 오래 살 수 없다"는 약속이다.
/// 원본 `Vec<u8>` 가 해제된 뒤에 payload 를 읽는 코드는 컴파일 자체가 안 된다.
#[derive(Debug, Clone, Copy)]
struct Chunk<'a> {
    fourcc: [u8; 4],
    payload: &'a [u8],
}

/// `webp` 에 ICC/EXIF 를 붙인 새 WebP 를 돌려준다. 둘 다 없으면 입력을 그대로 돌려준다(복사 0).
pub fn add_metadata(
    webp: Vec<u8>,
    width: u32,
    height: u32,
    icc: Option<&[u8]>,
    exif: Option<&[u8]>,
) -> Result<Vec<u8>> {
    if icc.is_none() && exif.is_none() {
        return Ok(webp);
    }

    let chunks = parse_chunks(&webp)?;

    let mut flags = 0u8;
    let mut image_chunks: Vec<Chunk> = Vec::new();
    for chunk in &chunks {
        match &chunk.fourcc {
            // 이미 VP8X 가 있으면(알파가 있는 손실 WebP) 알파 플래그만 이어받고 새로 쓴다
            b"VP8X" => flags |= chunk.payload.first().copied().unwrap_or(0) & FLAG_ALPHA,
            // 기존 메타데이터는 버리고 새 걸로 대체
            b"ICCP" | b"EXIF" | b"XMP " => {}
            b"ALPH" => {
                flags |= FLAG_ALPHA;
                image_chunks.push(*chunk);
            }
            _ => image_chunks.push(*chunk),
        }
    }
    if icc.is_some() {
        flags |= FLAG_ICC;
    }
    if exif.is_some() {
        flags |= FLAG_EXIF;
    }

    let mut out = Vec::with_capacity(
        webp.len() + 64 + icc.map_or(0, |b| b.len()) + exif.map_or(0, |b| b.len()),
    );
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&[0; 4]); // 전체 크기는 마지막에 채운다
    out.extend_from_slice(b"WEBP");

    // VP8X payload (10바이트): 플래그 1B + 예약 3B + (가로-1) 3B + (세로-1) 3B, 모두 리틀엔디언
    let mut vp8x = [0u8; 10];
    vp8x[0] = flags;
    vp8x[4..7].copy_from_slice(&u24_le(width - 1)?);
    vp8x[7..10].copy_from_slice(&u24_le(height - 1)?);
    write_chunk(&mut out, b"VP8X", &vp8x)?;

    // 청크 순서는 스펙이 정해 두었다: VP8X → ICCP → (ALPH) → VP8/VP8L → EXIF
    if let Some(icc) = icc {
        write_chunk(&mut out, b"ICCP", icc)?;
    }
    for chunk in &image_chunks {
        write_chunk(&mut out, &chunk.fourcc, chunk.payload)?;
    }
    if let Some(exif) = exif {
        write_chunk(&mut out, b"EXIF", exif)?;
    }

    let riff_size = u32::try_from(out.len() - 8).map_err(|_| too_big())?;
    out[4..8].copy_from_slice(&riff_size.to_le_bytes());
    Ok(out)
}

/// RIFF 헤더를 확인하고 청크 목록을 만든다. 각 청크의 payload 는 `webp` 를 빌린 슬라이스다.
fn parse_chunks(webp: &[u8]) -> Result<Vec<Chunk<'_>>> {
    if webp.len() < 12 || &webp[0..4] != b"RIFF" || &webp[8..12] != b"WEBP" {
        return Err(malformed("missing RIFF/WEBP header"));
    }

    let mut chunks = Vec::new();
    let mut rest = &webp[12..];
    while !rest.is_empty() {
        // split_at: 슬라이스를 두 조각의 슬라이스로 나눈다(복사 없이 경계만 정함)
        if rest.len() < 8 {
            return Err(malformed("truncated chunk header"));
        }
        let (header, body) = rest.split_at(8);
        let fourcc: [u8; 4] = header[0..4].try_into().expect("slice of len 4");
        let size = u32::from_le_bytes(header[4..8].try_into().expect("slice of len 4")) as usize;
        let padded = size + (size & 1);
        if body.len() < size {
            return Err(malformed("chunk larger than file"));
        }
        chunks.push(Chunk {
            fourcc,
            payload: &body[..size],
        });
        rest = &body[padded.min(body.len())..];
    }
    Ok(chunks)
}

fn write_chunk(out: &mut Vec<u8>, fourcc: &[u8; 4], payload: &[u8]) -> Result<()> {
    let size = u32::try_from(payload.len()).map_err(|_| too_big())?;
    out.extend_from_slice(fourcc);
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(payload);
    if payload.len() % 2 == 1 {
        out.push(0); // RIFF 청크는 짝수 길이로 맞춘다
    }
    Ok(())
}

fn u24_le(v: u32) -> Result<[u8; 3]> {
    if v >= 1 << 24 {
        return Err(malformed("canvas too large for VP8X"));
    }
    let b = v.to_le_bytes();
    Ok([b[0], b[1], b[2]])
}

fn malformed(why: &str) -> PixelVaultError {
    PixelVaultError::WebpEncode(format!("malformed webp container: {why}"))
}

fn too_big() -> PixelVaultError {
    PixelVaultError::WebpEncode("webp container exceeds 4 GiB".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, Rgb, RgbImage, Rgba, RgbaImage};

    fn lossy_webp(alpha: bool) -> Vec<u8> {
        let img = if alpha {
            DynamicImage::ImageRgba8(RgbaImage::from_pixel(16, 8, Rgba([10, 20, 30, 128])))
        } else {
            DynamicImage::ImageRgb8(RgbImage::from_pixel(16, 8, Rgb([10, 20, 30])))
        };
        crate::encode::encode(&img, crate::OutputFormat::Webp, 80).unwrap()
    }

    fn fourccs(webp: &[u8]) -> Vec<String> {
        parse_chunks(webp)
            .unwrap()
            .iter()
            .map(|c| String::from_utf8_lossy(&c.fourcc).into_owned())
            .collect()
    }

    #[test]
    fn no_metadata_returns_same_buffer() {
        let webp = lossy_webp(false);
        let ptr = webp.as_ptr();
        let out = add_metadata(webp, 16, 8, None, None).unwrap();
        assert_eq!(out.as_ptr(), ptr);
    }

    #[test]
    fn simple_webp_becomes_extended_with_exif() {
        let webp = lossy_webp(false);
        assert_eq!(fourccs(&webp), ["VP8 "]);

        let out = add_metadata(webp, 16, 8, None, Some(b"II*\0fake")).unwrap();
        assert_eq!(fourccs(&out), ["VP8X", "VP8 ", "EXIF"]);

        let vp8x = parse_chunks(&out).unwrap()[0].payload;
        assert_eq!(vp8x[0], FLAG_EXIF);
        assert_eq!(&vp8x[4..7], &[15, 0, 0]); // width - 1
        assert_eq!(&vp8x[7..10], &[7, 0, 0]); // height - 1

        // RIFF 크기 필드가 정확해야 디코더가 읽는다
        let riff = u32::from_le_bytes(out[4..8].try_into().unwrap()) as usize;
        assert_eq!(riff, out.len() - 8);
        image::load_from_memory(&out).expect("still a valid webp");
    }

    #[test]
    fn alpha_webp_keeps_alpha_flag_and_chunk_order() {
        let webp = lossy_webp(true);
        assert_eq!(fourccs(&webp), ["VP8X", "ALPH", "VP8 "]);

        let out = add_metadata(webp, 16, 8, Some(b"icc!"), Some(b"exif")).unwrap();
        assert_eq!(fourccs(&out), ["VP8X", "ICCP", "ALPH", "VP8 ", "EXIF"]);
        let flags = parse_chunks(&out).unwrap()[0].payload[0];
        assert_eq!(flags, FLAG_ICC | FLAG_ALPHA | FLAG_EXIF);

        let back = image::load_from_memory(&out).unwrap();
        assert!(back.color().has_alpha());
    }

    #[test]
    fn odd_sized_payload_is_padded() {
        let out = add_metadata(lossy_webp(false), 16, 8, None, Some(b"odd")).unwrap();
        assert_eq!(out.len() % 2, 0);
        assert_eq!(parse_chunks(&out).unwrap().last().unwrap().payload, b"odd");
    }

    #[test]
    fn rejects_non_webp() {
        assert!(add_metadata(b"not a webp".to_vec(), 1, 1, None, Some(b"x")).is_err());
    }
}
