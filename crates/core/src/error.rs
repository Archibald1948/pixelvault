//! 파이프라인 전체에서 쓰는 에러 타입.
//!
//! `thiserror` 의 `#[derive(Error)]` 가 `std::error::Error` 와 `Display` 구현을 만들어 준다.
//! 각 단계 함수는 `Result<T, PixelVaultError>` 를 돌려주고, 호출하는 쪽은 `?` 로 위로 전파한다.
//! wasm 크레이트에서는 이 에러가 `JsError` 로 바뀌어 JS 의 `Error` 로 throw 된다.

/// `Result<T, PixelVaultError>` 를 줄여 쓰기 위한 별칭.
pub type Result<T> = std::result::Result<T, PixelVaultError>;

#[derive(Debug, thiserror::Error)]
pub enum PixelVaultError {
    /// 매직 바이트로 포맷을 알아낼 수 없거나, 지원하지 않는 포맷
    #[error("unsupported input format (supported: JPEG, PNG, WebP)")]
    UnsupportedInput,

    /// 디코드 전에 크기를 계산해 보니 메모리 상한을 넘는 경우
    #[error(
        "image too large: {width}x{height} would need {needed_bytes} bytes decoded (limit {limit_bytes})"
    )]
    TooLarge {
        width: u32,
        height: u32,
        needed_bytes: u64,
        limit_bytes: u64,
    },

    /// 옵션 값이 범위를 벗어난 경우 (quality 0, 모르는 format 문자열 등)
    #[error("invalid option: {0}")]
    InvalidOption(String),

    /// `image` 크레이트가 낸 디코드/인코드 에러. `#[from]` 덕분에 `?` 로 자동 변환된다.
    #[error("image codec error: {0}")]
    Codec(#[from] image::ImageError),

    #[error("resize failed: {0}")]
    Resize(#[from] fast_image_resize::ResizeError),

    #[error("jpeg encode failed: {0}")]
    JpegEncode(#[from] jpeg_encoder::EncodingError),

    /// libwebp 인코더 에러 (webp 크레이트의 에러 타입은 Error 트레잇을 구현하지 않아 문자열로 받는다)
    #[error("webp encode failed: {0}")]
    WebpEncode(String),
}
