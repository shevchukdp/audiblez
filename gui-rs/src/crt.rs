//! Minimal CRT compatibility stubs for the freestanding `gnullvm` link.
//!
//! gnullvm's bundled CRT object `crt2.o` (which defines `mainCRTStartup` /
//! `__tmainCRTStartup`, the entry point that Rust `fn main` maps to) references
//! a couple of symbols that the toolchain's bundled import libraries do *not*
//! ship — we verified `__p___initenv` and `_crt_atexit` are absent from every
//! archive in `self-contained`.
//!
//! In a normal mingw-w64 build these come from `misc/environ.obj`, which
//! defines `char **__initenv;` and `char ***__p___initenv = &__initenv;`.
//! gnullvm ships neither, so `__tmainCRTStartup`'s
//!     char **envp = *__p___initenv;
//! dereferenced a NULL stub and crashed with an access violation
//! (STATUS_ACCESS_VIOLATION / 0xC0000005) long before `main` ran. The fix is to
//! mirror `environ.obj`: give `__p___initenv` a writable slot that already
//! points at a valid, NULL-terminated (empty) environment. A GUI launcher does
//! not need the real environment or atexit cleanup, so the contents can be
//! empty — but the *pointer* must be valid.
//!
//! All three are `static mut`: raw pointers are not `Sync`, so they cannot live
//! in an immutable `static`, and `__crt_initenv` must be writable because
//! `__tmainCRTStartup` stores the real environment pointer there.

use core::ffi::c_void;

/// A valid, empty environment block: a `char**` array terminated by a single
/// NULL. crt2.o iterates `*__p___initenv`, so it must be non-NULL and
/// NULL-terminated; an array with just a NULL entry means "empty environment".
#[no_mangle]
pub static mut __crt_empty_envp: [*const c_void; 1] = [core::ptr::null()];

/// `char **__initenv` — mirrors mingw-w64's `environ.obj`. `static mut` because
/// `__tmainCRTStartup` may store the real environment pointer here at startup.
/// Initialized to point at our valid empty environment (never NULL).
#[no_mangle]
pub static mut __crt_initenv: *const [*const c_void; 1] = core::ptr::addr_of!(__crt_empty_envp);

/// `char ***__p___initenv = &__initenv` — the symbol crt2.o actually
/// references. Pointing it at the writable `__crt_initenv` slot (which already
/// holds a valid empty environment) means `*__p___initenv` is never NULL.
#[no_mangle]
pub static mut __p___initenv: *mut *const [*const c_void; 1] =
    core::ptr::addr_of_mut!(__crt_initenv);

/// `atexit` registration hook used by `__tmainCRTStartup`. We ignore the
/// registered handlers — no atexit cleanup runs, which is fine for this
/// launcher. Returns 0 (success), matching the C contract.
#[no_mangle]
pub extern "C" fn _crt_atexit(_func: extern "C" fn() -> ()) -> i32 {
    0
}
