//! Windows DPAPI, current-user scope, without UI: [`protect`] and [`unprotect`].
//!
//! This is the workspace's only `unsafe` code (the workspace forbids it everywhere else). It is
//! limited to the two DPAPI calls and freeing their output, and compiles to nothing on other
//! operating systems.
#![cfg(windows)]

use std::{ptr, slice};
use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
    },
};

/// DPAPI refused the operation: wrong user, wrong entropy, damaged data or a system failure.
/// Details are deliberately not surfaced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Failed;

/// Encrypts `data` for the current Windows user. `entropy` must be given again to decrypt.
pub fn protect(data: &[u8], entropy: &[u8]) -> Result<Vec<u8>, Failed> {
    call(data, entropy, true)
}

/// Decrypts what [`protect`] produced for the current Windows user with the same `entropy`.
pub fn unprotect(data: &[u8], entropy: &[u8]) -> Result<Vec<u8>, Failed> {
    call(data, entropy, false)
}

fn blob(bytes: &[u8]) -> Result<CRYPT_INTEGER_BLOB, Failed> {
    Ok(CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(bytes.len()).map_err(|_| Failed)?,
        pbData: bytes.as_ptr().cast_mut(),
    })
}

#[allow(unsafe_code)]
fn call(data: &[u8], entropy: &[u8], seal: bool) -> Result<Vec<u8>, Failed> {
    let input = blob(data)?;
    let entropy = blob(entropy)?;
    let mut output = CRYPT_INTEGER_BLOB::default();
    // SAFETY: `input` and `entropy` point at slices that outlive the call and DPAPI only reads
    // them; the description, reserved and prompt pointers are null as documented; `output` is
    // a valid out-parameter.
    let ok = unsafe {
        if seal {
            CryptProtectData(
                &input,
                ptr::null(),
                &entropy,
                ptr::null(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                ptr::null_mut(),
                &entropy,
                ptr::null(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    if ok == 0 {
        return Err(Failed);
    }
    if output.pbData.is_null() {
        return Ok(Vec::new());
    }
    // SAFETY: on success DPAPI returns `cbData` initialised bytes at `pbData`, allocated with
    // LocalAlloc and owned by the caller; they are copied, wiped and freed exactly once here.
    unsafe {
        let bytes = slice::from_raw_parts_mut(output.pbData, output.cbData as usize);
        let copy = bytes.to_vec();
        bytes
            .iter_mut()
            .for_each(|byte| ptr::write_volatile(byte, 0));
        LocalFree(output.pbData.cast());
        Ok(copy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_only_with_the_same_entropy() {
        let sealed = protect(b"secret", b"scope-a").unwrap();
        assert!(!sealed.windows(6).any(|window| window == b"secret"));
        assert_eq!(unprotect(&sealed, b"scope-a").unwrap(), b"secret");
        assert_eq!(unprotect(&sealed, b"scope-b"), Err(Failed));
        assert_eq!(unprotect(b"not dpapi", b"scope-a"), Err(Failed));
    }
}
