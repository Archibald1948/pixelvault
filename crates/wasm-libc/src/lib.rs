//! pixelvault-wasm-libc — wasm32 에서 libwebp(C) 가 링크될 수 있게 해 주는 최소 libc.
//!
//! `wasm32-unknown-unknown` 타깃에는 C 표준 라이브러리가 없다. 그래서 C 로 작성된
//! libwebp 를 wasm 으로 빌드하면 `malloc`, `free`, `qsort` 같은 심볼이 비어 있는 채로 남는다.
//! 이 크레이트는 그 심볼들을 Rust 로 구현해서 채워 준다.
//!
//! - 헤더(선언)는 `include/*.h` 에 있다. `.cargo/config.toml` 이 `C_INCLUDE_PATH` 로 연결한다.
//! - `memcpy`/`memset`/수학 함수 등은 Rust 의 `compiler_builtins` 가 이미 제공하므로 여기엔 없다.
//! - wasm32 가 아닌 타깃에서는 아무것도 컴파일되지 않는다 (진짜 libc 가 있으니까).
//!
//! 자세한 배경은 `docs/libwebp-on-wasm.md` 참고.

#![cfg(target_arch = "wasm32")]

use core::ffi::{c_int, c_void};
use std::alloc::{Layout, alloc, alloc_zeroed, dealloc, realloc as rust_realloc};
use std::cmp::Ordering;

/// C 의 `malloc` 은 크기를 기억하지 않고 `free(ptr)` 만 받지만,
/// Rust 의 `dealloc` 은 할당할 때의 `Layout`(크기+정렬)을 다시 요구한다.
/// 그래서 실제 할당은 `HEADER` 바이트만큼 더 크게 하고, 앞부분에 전체 크기를 적어 둔다.
///
/// ```text
/// [ total_size: usize | (패딩) ][ 사용자에게 돌려주는 영역 ... ]
/// ^ base                         ^ base + HEADER  (= malloc 의 반환값)
/// ```
///
/// 16 바이트로 잡으면 C 가 기대하는 `max_align_t` 정렬도 동시에 만족한다.
const HEADER: usize = 16;

fn layout_for(user_size: usize) -> Option<Layout> {
    let total = user_size.checked_add(HEADER)?;
    Layout::from_size_align(total, HEADER).ok()
}

/// 할당된 블록 앞에 크기를 기록하고, 사용자 영역 포인터를 돌려준다.
///
/// # Safety
/// `base` 는 `layout` 으로 막 할당된, null 이 아닌 포인터여야 한다.
unsafe fn finish_alloc(base: *mut u8, layout: Layout) -> *mut c_void {
    unsafe {
        (base as *mut usize).write(layout.size());
        base.add(HEADER).cast()
    }
}

/// 사용자 포인터에서 원래 블록의 시작 주소와 Layout 을 복원한다.
///
/// # Safety
/// `ptr` 은 이 모듈의 `malloc`/`calloc`/`realloc` 이 돌려준 포인터여야 한다.
unsafe fn block_of(ptr: *mut c_void) -> (*mut u8, Layout) {
    unsafe {
        let base = (ptr as *mut u8).sub(HEADER);
        let total = (base as *const usize).read();
        (base, Layout::from_size_align_unchecked(total, HEADER))
    }
}

/// C 의 `malloc`.
///
/// # Safety
/// C 코드에서만 호출된다. 돌려준 포인터는 반드시 이 모듈의 `free`/`realloc` 으로만 해제해야 한다.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn malloc(size: usize) -> *mut c_void {
    let Some(layout) = layout_for(size) else {
        return core::ptr::null_mut();
    };
    unsafe {
        let base = alloc(layout);
        if base.is_null() {
            return core::ptr::null_mut();
        }
        finish_alloc(base, layout)
    }
}

/// C 의 `calloc` (0 으로 초기화된 `n * size` 바이트).
///
/// # Safety
/// `malloc` 과 같다.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn calloc(n: usize, size: usize) -> *mut c_void {
    // n * size 가 오버플로하면 C 표준대로 NULL 을 돌려준다.
    let Some(layout) = n.checked_mul(size).and_then(layout_for) else {
        return core::ptr::null_mut();
    };
    unsafe {
        let base = alloc_zeroed(layout);
        if base.is_null() {
            return core::ptr::null_mut();
        }
        finish_alloc(base, layout)
    }
}

/// C 의 `realloc`.
///
/// # Safety
/// `ptr` 은 null 이거나 이 모듈이 할당한, 아직 해제되지 않은 포인터여야 한다.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn realloc(ptr: *mut c_void, size: usize) -> *mut c_void {
    unsafe {
        if ptr.is_null() {
            return malloc(size);
        }
        if size == 0 {
            free(ptr);
            return core::ptr::null_mut();
        }
        let Some(new_layout) = layout_for(size) else {
            return core::ptr::null_mut();
        };
        let (base, old_layout) = block_of(ptr);
        let new_base = rust_realloc(base, old_layout, new_layout.size());
        if new_base.is_null() {
            return core::ptr::null_mut();
        }
        finish_alloc(new_base, new_layout)
    }
}

/// C 의 `free`.
///
/// # Safety
/// `ptr` 은 null 이거나 이 모듈이 할당한, 아직 해제되지 않은 포인터여야 한다. (이중 해제 금지)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn free(ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }
    unsafe {
        let (base, layout) = block_of(ptr);
        dealloc(base, layout);
    }
}

type CompareFn = unsafe extern "C" fn(*const c_void, *const c_void) -> c_int;

/// C 의 `qsort`. libwebp 는 팔레트/허프만 트리 정렬(무손실 인코딩 경로)에서 쓴다.
///
/// 원소 크기(`size`)가 런타임 값이라 Rust 의 제네릭 `sort` 를 그대로 쓸 수 없다.
/// 대신 "인덱스 배열"을 정렬한 뒤, 그 순서대로 바이트를 다시 배치한다.
///
/// # Safety
/// `base` 는 `n * size` 바이트의 유효한 배열이어야 하고, `cmp` 는 전순서(total order)여야 한다.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qsort(base: *mut c_void, n: usize, size: usize, cmp: CompareFn) {
    if n < 2 || size == 0 {
        return;
    }
    unsafe {
        let bytes = core::slice::from_raw_parts_mut(base as *mut u8, n * size);
        let original = bytes.to_vec();
        let elem = |i: usize| original.as_ptr().add(i * size).cast::<c_void>();

        let mut order: Vec<usize> = (0..n).collect();
        order.sort_unstable_by(|&a, &b| cmp(elem(a), elem(b)).cmp(&0));

        for (dst, &src) in order.iter().enumerate() {
            bytes[dst * size..(dst + 1) * size]
                .copy_from_slice(&original[src * size..(src + 1) * size]);
        }
    }
}

/// C 의 `bsearch`. 정렬된 배열에서 이진 탐색.
///
/// # Safety
/// `base` 는 `cmp` 기준으로 정렬된 `n * size` 바이트의 유효한 배열이어야 한다.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bsearch(
    key: *const c_void,
    base: *const c_void,
    n: usize,
    size: usize,
    cmp: CompareFn,
) -> *mut c_void {
    let (mut lo, mut hi) = (0usize, n);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let elem = unsafe { (base as *const u8).add(mid * size) }.cast::<c_void>();
        match unsafe { cmp(key, elem) }.cmp(&0) {
            Ordering::Less => hi = mid,
            Ordering::Greater => lo = mid + 1,
            Ordering::Equal => return elem.cast_mut(),
        }
    }
    core::ptr::null_mut()
}

#[unsafe(no_mangle)]
pub extern "C" fn abort() -> ! {
    std::process::abort()
}
