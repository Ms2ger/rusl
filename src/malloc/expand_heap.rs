use core::num::Wrapping;
use core::ptr;
use core::usize;

use c_types::*;
use environ::AUXV_PTR;
use errno::{set_errno, ENOMEM};
use platform::mman::*;
use mmap::__mmap;

/// Comment from original musl C function:
///
/// This function returns true if the interval [old,new]
/// intersects the 'len'-sized interval below &libc.auxv
/// (interpreted as the main-thread stack) or below &b
/// (the current stack). It is used to defend against
/// buggy brk implementations that can cross the stack.
#[no_mangle]
pub extern "C" fn traverses_stack_p(old: usize, new: usize) -> c_int {

    let len = 8usize << 20;

    let b = *AUXV_PTR;
    let a = if b > len {
        b - len
    } else {
        0
    };

    if new > a && old < b {
        return 1;
    }

    let b = (&b as *const usize) as usize;
    let a = if b > len {
        b - len
    } else {
        0
    };

    if new > a && old < b {
        return 1;
    }

    0
}

// Expand the heap in-place if brk can be used, or otherwise via mmap,
// using an exponential lower bound on growth by mmap to make
// fragmentation asymptotically irrelevant. The size argument is both
// an input and an output, since the caller needs to know the size
// allocated, which will be larger than requested due to page alignment
// and mmap minimum size rules. The caller is responsible for locking
// to prevent concurrent calls.

#[no_mangle]
pub unsafe extern "C" fn __expand_heap(pn: *mut size_t) -> *mut c_void {
    static mut brk: usize = 0;
    static mut mmap_step: c_uint = 0;

    let mut n = *pn;

    if n > ((usize::MAX / 2) - PAGE_SIZE as usize) {
        set_errno(ENOMEM);
        return ptr::null_mut();
    }

    n += (-Wrapping(n)).0 & (PAGE_SIZE as usize - 1);

    if brk == 0 {
        brk = syscall!(BRK, 0);
        brk += (-Wrapping(brk)).0 & (PAGE_SIZE - 1) as usize;
    }

    if n < (usize::MAX - brk) && traverses_stack_p(brk, brk + n) == 0 &&
       syscall!(BRK, brk + n) == brk + n {
        *pn = n;
        brk += n;
        return (brk - n) as *mut c_void;
    }

    let min = (PAGE_SIZE << (mmap_step / 2)) as usize;

    if n < min {
        n = min;
    }

    let area = __mmap(ptr::null_mut(),
                      n,
                      PROT_READ | PROT_READ,
                      MAP_PRIVATE | MAP_ANONYMOUS,
                      -1,
                      0);

    if area == MAP_FAILED {
        return ptr::null_mut();
    }

    *pn = n;
    mmap_step += 1;

    area
}
