pub mod blurhash;
pub mod decode;
pub mod encode;
pub mod error;
pub mod exif;
pub mod pipeline;
pub mod resize;
pub mod webp_meta;

pub use encode::OutputFormat;
pub use error::{PixelVaultError, Result};
pub use pipeline::{ProcessOptions, ProcessOutput, process};

// wasm32 에서는 libwebp 가 쓸 malloc/free 등을 이 크레이트가 제공한다.
// 코드에서 직접 부르는 함수가 없어서, 이렇게 명시하지 않으면 링크 대상에서 빠진다.
#[cfg(target_arch = "wasm32")]
extern crate pixelvault_wasm_libc as _;

/// M0 용 헬로월드 함수. 빌드 → 번들 → 로드 경로가 뚫렸는지 확인하는 용도.
pub fn add(a: u32, b: u32) -> u32 {
    a + b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_works() {
        assert_eq!(add(2, 3), 5);
    }
}
