//! Native Windows file/folder pickers.
//!
//! egui has no native file dialogs, and this app is Windows-only, so we call
//! into the system `commdlg` (`GetOpenFileNameW` / `GetSaveFileNameW`, via the
//! modern `windows` crate) for file selection and shell out to Explorer for
//! folder selection.

use std::ffi::OsString;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::ptr::null_mut;

use windows::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, GetSaveFileNameW, OPENFILENAMEW,
};
use windows::Win32::UI::WindowsAndMessaging::MessageBoxW;

/// Turn a `&str` into a null-terminated UTF-16 buffer for Win32 string params.
fn wstr(s: &str) -> Vec<u16> {
    std::ffi::OsString::from(s)
        .encode_wide()
        .chain(std::iter::once(0u16))
        .collect()
}

/// Convert a Windows-UTF-16 buffer into a Rust `String`.
unsafe fn buf_to_string(buf: *const u16, cap: usize) -> String {
    let mut len = 0usize;
    while len < cap && *buf.add(len) != 0 {
        len += 1;
    }
    OsString::from_wide(std::slice::from_raw_parts(buf, len))
        .to_string_lossy()
        .into_owned()
}

/// Show a native "Open File" dialog and return the selected path (if any).
pub fn open_file(title: &str) -> Option<String> {
    // The buffer that receives the chosen filename. It must hold enough room
    // for the initial (empty) filter + result, plus the null terminator.
    let mut file_buffer = vec![0u16; 260];
    // Strings must stay alive for the duration of the dialog call.
    let title_w = wstr(title);
    let mut ofn: OPENFILENAMEW = OPENFILENAMEW::default();
    ofn.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    ofn.hwndOwner = windows::Win32::Foundation::HWND(0);
    ofn.lpstrFilter = windows::core::PCWSTR(null_mut());
    ofn.lpstrFile = windows::core::PWSTR(file_buffer.as_mut_ptr());
    ofn.nMaxFile = file_buffer.len() as u32;
    ofn.lpstrFileTitle = windows::core::PWSTR(null_mut());
    ofn.nMaxFileTitle = 0;
    ofn.lpstrInitialDir = windows::core::PCWSTR(null_mut());
    ofn.lpstrTitle = windows::core::PCWSTR(title_w.as_ptr());
    ofn.Flags = windows::Win32::UI::Controls::Dialogs::OPEN_FILENAME_FLAGS::default();
    let r = unsafe { GetOpenFileNameW(&mut ofn as *mut OPENFILENAMEW) };
    if r.as_bool() {
        unsafe { Some(buf_to_string(file_buffer.as_ptr(), file_buffer.len())) }
    } else {
        None // ERROR_CANCELLED (user cancelled) or FALSE (system error)
    }
}

/// Show a native "Save File" dialog and return the chosen path (if any).
pub fn save_file(title: &str, initial_dir: &str) -> Option<String> {
    let mut file_buffer = vec![0u16; 260];
    let title_w = wstr(title);
    let initial_w = wstr(initial_dir);
    let mut ofn: OPENFILENAMEW = OPENFILENAMEW::default();
    ofn.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    ofn.hwndOwner = windows::Win32::Foundation::HWND(0);
    ofn.lpstrFilter = windows::core::PCWSTR(null_mut());
    ofn.lpstrFile = windows::core::PWSTR(file_buffer.as_mut_ptr());
    ofn.nMaxFile = file_buffer.len() as u32;
    ofn.lpstrFileTitle = windows::core::PWSTR(null_mut());
    ofn.nMaxFileTitle = 0;
    ofn.lpstrInitialDir = windows::core::PCWSTR(initial_w.as_ptr());
    ofn.lpstrTitle = windows::core::PCWSTR(title_w.as_ptr());
    ofn.Flags = windows::Win32::UI::Controls::Dialogs::OPEN_FILENAME_FLAGS::default();
    let r = unsafe { GetSaveFileNameW(&mut ofn as *mut OPENFILENAMEW) };
    if r.as_bool() {
        unsafe { Some(buf_to_string(file_buffer.as_ptr(), file_buffer.len())) }
    } else {
        None
    }
}

/// Open a folder in Windows Explorer.
pub fn open_folder(path: &str) {
    use std::process::Command;
    let _ = Command::new("explorer").arg(path).spawn();
}

/// Show a system error message box.
pub fn error_message(title: &str, message: &str) {
    let msg = wstr(message);
    let ttl = wstr(title);
    unsafe {
        MessageBoxW(
            windows::Win32::Foundation::HWND(0),
            windows::core::PCWSTR(msg.as_ptr()),
            windows::core::PCWSTR(ttl.as_ptr()),
            windows::Win32::UI::WindowsAndMessaging::MB_ICONINFORMATION,
        );
    }
}
