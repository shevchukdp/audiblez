//! Minimal CRT compatibility stubs for the freestanding `gnullvm` link.
//!
//! gnullvm's bundled CRT object `crt2.o` (which defines `mainCRTStartup` /
//! `__tmainCRTStartup`, the entry point that Rust `fn main` maps to) references
//! a couple of symbols that the toolchain's bundled import libraries do *not*
//! ship — we verified `__p___initenv` and `_crt_atexit` are absent from every
//! archive in `self-contained` (`libmsvcrt.a` only has `atexit`/`_atexit` and
//! `__initenv`, not the pointer `_crt_atexit` nor `__p___initenv`).
//!
//! Rather than depend on a system C runtime, we provide self-contained
//! stand-ins here. A GUI launcher does not need environment handling or
//! atexit cleanup to run, so these can safely be no-ops.

/// Address of the environment block (`envp`) as expected by the CRT startup.
/// A single NULL pointer is a valid, empty environment.
#[no_mangle]
pub static mut __p___initenv: *mut *const u8 = std::ptr::null_mut();

/// `atexit` registration hook used by `__tmainCRTStartup`. We ignore the
/// registered handlers — no atexit cleanup runs, which is fine for this
/// launcher. Returns 0 (success), matching the C contract.
#[no_mangle]
pub extern "C" fn _crt_atexit(_func: extern "C" fn() -> ()) -> i32 {
    0
}
