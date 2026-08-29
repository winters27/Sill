//! Keeping credentials out of a settings file.
//!
//! A cloud transcription key was written into `preferences.json` as plain
//! text, next to the base URL, in a directory that backup and sync tools copy
//! by default. Anyone who ever read that file, or a copy of it, had the key.
//!
//! **What this fixes and what it does not.** DPAPI encrypts with a key derived
//! from the user's Windows logon, so the sealed value is useless on another
//! machine, in a backup, in a synced folder, or to another account on this
//! one. It is *not* a defence against a process already running as this user:
//! such a process can simply call the unseal side, exactly as Sill does. That
//! is the honest boundary, and no design that keeps a usable key on a desktop
//! machine can do better without asking for a passphrase on every dictation.
//!
//! Declared by hand rather than by enabling the `windows` crate's
//! `Win32_Security_Cryptography` feature. The same reasoning as `icons.rs`:
//! this crate's feature list has already pushed rustc into an out-of-memory
//! abort once by accumulating, and two extern declarations cost nothing.

/// Marks a value this module sealed, and says which scheme did it.
///
/// A prefix rather than a wrapper object, so migration needs no schema
/// change: a value without it is a key written by an older build, read as-is
/// and re-sealed the next time preferences are saved.
const PREFIX: &str = "dpapi:v1:";

/// Mixed into the encryption so a sealed value is bound to this application.
///
/// Worth little on its own, since it is a constant inside a binary anyone can
/// read. It is here because DPAPI offers it and it costs nothing, not because
/// it changes the boundary described above.
const ENTROPY: &[u8] = b"app.winters.sill/dictation-provider";

/// Whether a stored value has already been sealed.
pub fn is_sealed(value: &str) -> bool {
    value.starts_with(PREFIX)
}

#[cfg(windows)]
mod windows_impl {
    use base64::Engine;

    use super::{ENTROPY, PREFIX};

    /// Mirrors Win32's `DATA_BLOB`.
    #[repr(C)]
    struct DataBlob {
        cb_data: u32,
        pb_data: *mut u8,
    }

    /// Do not prompt, ever. A dialog raised from a background thread during a
    /// dictation would be a hang with no explanation.
    const CRYPTPROTECT_UI_FORBIDDEN: u32 = 0x1;

    #[link(name = "crypt32")]
    extern "system" {
        fn CryptProtectData(
            data_in: *const DataBlob,
            description: *const u16,
            optional_entropy: *const DataBlob,
            reserved: *mut core::ffi::c_void,
            prompt: *mut core::ffi::c_void,
            flags: u32,
            data_out: *mut DataBlob,
        ) -> i32;

        fn CryptUnprotectData(
            data_in: *const DataBlob,
            description: *mut *mut u16,
            optional_entropy: *const DataBlob,
            reserved: *mut core::ffi::c_void,
            prompt: *mut core::ffi::c_void,
            flags: u32,
            data_out: *mut DataBlob,
        ) -> i32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn LocalFree(mem: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
    }

    fn blob(bytes: &[u8]) -> DataBlob {
        DataBlob {
            cb_data: bytes.len() as u32,
            // Cast of a shared reference, which is sound only because both
            // DPAPI calls treat the input blob as read-only.
            pb_data: bytes.as_ptr() as *mut u8,
        }
    }

    /// Copies what DPAPI allocated and hands its memory back.
    ///
    /// The output blob is `LocalAlloc`ed by Windows and leaks if it is not
    /// freed. Doing that here means neither caller has to remember.
    unsafe fn take(out: DataBlob) -> Vec<u8> {
        let copied = std::slice::from_raw_parts(out.pb_data, out.cb_data as usize).to_vec();
        LocalFree(out.pb_data as *mut core::ffi::c_void);
        copied
    }

    pub fn seal(plaintext: &str) -> Option<String> {
        let input = blob(plaintext.as_bytes());
        let entropy = blob(ENTROPY);
        let mut out = DataBlob {
            cb_data: 0,
            pb_data: std::ptr::null_mut(),
        };

        // SAFETY: both input blobs point at live slices for the duration of
        // the call, and `out` is a valid writable blob. The result is checked
        // before its contents are read.
        let ok = unsafe {
            CryptProtectData(
                &input,
                std::ptr::null(),
                &entropy,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        };

        if ok == 0 {
            return None;
        }

        // SAFETY: the call succeeded, so `out` owns a LocalAlloc'ed buffer.
        let sealed = unsafe { take(out) };
        Some(format!(
            "{PREFIX}{}",
            base64::engine::general_purpose::STANDARD.encode(sealed)
        ))
    }

    pub fn unseal(sealed: &str) -> Option<String> {
        let encoded = sealed.strip_prefix(PREFIX)?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .ok()?;

        let input = blob(&bytes);
        let entropy = blob(ENTROPY);
        let mut out = DataBlob {
            cb_data: 0,
            pb_data: std::ptr::null_mut(),
        };

        // SAFETY: as above. The description pointer is null because the
        // description is not wanted, which DPAPI documents as allowed.
        let ok = unsafe {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                &entropy,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        };

        if ok == 0 {
            return None;
        }

        // SAFETY: the call succeeded, so `out` owns a LocalAlloc'ed buffer.
        let plaintext = unsafe { take(out) };
        String::from_utf8(plaintext).ok()
    }
}

#[cfg(windows)]
pub use windows_impl::{seal, unseal};

/// No DPAPI off Windows, so nothing is sealed and nothing pretends to be.
///
/// Returning `None` rather than the plaintext is deliberate: a caller that
/// stores whatever comes back would otherwise write an unsealed value under a
/// name that says it is sealed.
#[cfg(not(windows))]
pub fn seal(_plaintext: &str) -> Option<String> {
    None
}

#[cfg(not(windows))]
pub fn unseal(_sealed: &str) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_that_was_never_sealed_is_not_mistaken_for_one() {
        // The migration case. A key written by an older build has to be read
        // as itself rather than run through the decrypt path and lost.
        assert!(!is_sealed("sk-live-abc123"));
        assert!(!is_sealed(""));
        assert!(is_sealed("dpapi:v1:AQAAAA=="));
    }

    #[cfg(windows)]
    #[test]
    fn a_sealed_key_comes_back_exactly() {
        let key = "sk-proj-Abc123-XYZ_the/quick+brown";
        let sealed = seal(key).expect("DPAPI is available on Windows");

        assert!(is_sealed(&sealed), "the marker prefix is missing");
        assert!(
            !sealed.contains(key),
            "the plaintext survived into the sealed form"
        );
        assert_eq!(unseal(&sealed).as_deref(), Some(key));
    }

    #[cfg(windows)]
    #[test]
    fn sealing_the_same_value_twice_does_not_produce_the_same_text() {
        // DPAPI salts each call. Identical ciphertext for identical input
        // would leak that two providers share a key.
        let a = seal("same-key").expect("sealed");
        let b = seal("same-key").expect("sealed");
        assert_ne!(a, b);
        assert_eq!(unseal(&a), unseal(&b));
    }

    #[cfg(windows)]
    #[test]
    fn a_corrupt_or_foreign_value_fails_rather_than_returning_rubbish() {
        assert_eq!(unseal("dpapi:v1:not-base64!!"), None);
        assert_eq!(unseal("dpapi:v1:AQAAAA=="), None, "truncated blob");
        assert_eq!(unseal("plain text"), None, "no prefix, so not ours");
    }

    #[cfg(windows)]
    #[test]
    fn an_empty_key_round_trips_rather_than_erroring() {
        // Clearing a key is a normal thing to do, and it must not leave the
        // previous one behind because the empty case failed.
        let sealed = seal("").expect("sealed");
        assert_eq!(unseal(&sealed).as_deref(), Some(""));
    }
}
