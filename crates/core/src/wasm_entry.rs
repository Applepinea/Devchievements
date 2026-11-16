//! Minimal C-ABI surface for the wasm32 build (ADR-0001 spike).
//!
//! The browser side instantiates this module and calls `devch_probe_len` then
//! `devch_probe_write` to copy the probe JSON into its own memory. That is enough
//! to prove instantiation + invocation on wasm32 without any JS glue.
//!
//! `#[no_mangle]` is wasm-specific plumbing, not business logic. This is the only
//! place in the crate allowed to use `unsafe`, and only for the documented
//! pointer copy below.
#![allow(unsafe_code)]

/// Length in bytes of the probe payload.
#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn devch_probe_len() -> u32 {
    crate::wasm_probe::sample_json().len() as u32
}

/// Copy the probe payload into `ptr`.
///
/// # Safety
///
/// The caller must guarantee `ptr` points to at least `devch_probe_len()` writable
/// bytes in this module's linear memory. This is the standard wasm hand-off and
/// is the only unsafe block in the crate.
#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub unsafe extern "C" fn devch_probe_write(ptr: *mut u8) -> u32 {
    let json = crate::wasm_probe::sample_json();
    let len = json.len();
    // SAFETY: the caller contract above states ptr has room for len bytes.
    unsafe {
        std::ptr::copy_nonoverlapping(json.as_ptr(), ptr, len);
    }
    len as u32
}
