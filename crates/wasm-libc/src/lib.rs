use core::ffi::{c_int, c_void};
use std::alloc::{Layout, alloc, alloc_zeroed, dealloc, realloc as rust_realloc};
use std::cmp::Ordering;

/// 16 바이트로 잡으면 C 가 기대하는 `max_align_t` 정렬도 동시에 만족한다.
const HEADER: usize = 16;

fn layout_for(user_size: usize) -> Option<Layout> {
    let total = user_size.checked_add(HEADER)?;
    Layout::from_size_align(total, HEADER).ok()
}

/// 할당된 블록 앞에 크기를 기록하고, 사용자 영역 포인터를 돌려준다.
unsafe fn finish_alloc(base: *mut u8, layout: Layout) -> *mut c_void {
    unsafe {
        (base as *mut usize).write(layout.size());
        base.add(HEADER).cast()
    }
}

/// 사용자 포인터에서 원래 블록의 시작 주소와 Layout 을 복원한다.
unsafe fn block_of(ptr: *mut c_void) -> (*mut u8, Layout) {
    unsafe {
        let base = (ptr as *mut u8).sub(HEADER);
        let total = (base as *const usize).read();
        (base, Layout::from_size_align_unchecked(total, HEADER))
    }
}

/// C 코드에서만 호출된다. 돌려준 포인터는 반드시 이 모듈의 `free`/`realloc` 으로만 해제해야 한다.
///
/// # Safety
/// 호출자는 반환된 포인터를 이 모듈이 제공하는 `free`/`realloc` 으로만 해제해야 하며,
/// 이미 해제된 포인터를 다시 넘기면 안 된다.
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
/// 호출자는 반환된 포인터를 이 모듈의 `free`/`realloc` 으로만 다뤄야 하며,
/// 해제 후 재사용하거나 이중 해제를 시도하면 안 된다.
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

/// `ptr` 은 null 이거나 이 모듈이 할당한, 아직 해제되지 않은 포인터여야 한다.
///
/// # Safety
/// `ptr` 이 null 이 아니라면 반드시 이 모듈의 `malloc`/`calloc`/`realloc` 으로
/// 얻은 유효한 포인터여야 하며, 이미 해제된 포인터여서는 안 된다.
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

/// `ptr` 은 null 이거나 이 모듈이 할당한, 아직 해제되지 않은 포인터여야 한다. (이중 해제 금지)
///
/// # Safety
/// `ptr` 이 null 이 아니라면 반드시 이 모듈의 할당 함수가 반환한 포인터여야 하며,
/// 한 번 해제한 포인터를 다시 넘기면 안 된다.
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

/// `base` 는 `n * size` 바이트의 유효한 배열이어야 하고, `cmp` 는 전순서(total order)여야 한다.
///
/// # Safety
/// `base` 는 최소 `n * size` 바이트의 쓰기 가능한 유효 메모리를 가리켜야 하고,
/// `cmp` 는 전달된 원소 포인터를 안전하게 역참조할 수 있어야 하며 일관된 전순서를 제공해야 한다.
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

/// `base` 는 `cmp` 기준으로 정렬된 `n * size` 바이트의 유효한 배열이어야 한다.
///
/// # Safety
/// `key` 와 `base` 는 `cmp` 가 요구하는 방식으로 유효하게 읽을 수 있어야 하며,
/// `base` 는 최소 `n * size` 바이트의 연속된 메모리를 가리켜야 한다.
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
