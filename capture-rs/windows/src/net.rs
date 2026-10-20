// context-capsule — windows: https downloads through WinINet (Windows' own, so proxies and certificates follow
// the system settings) — used by the speech model download and by updates.

#[cfg(windows)]
pub use win::{fetch, WinInet};

#[cfg(windows)]
mod win {
    use std::io::Write;
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};
    use windows::core::{w, HSTRING};
    use windows::Win32::Networking::WinInet::{
        HttpQueryInfoW, InternetCloseHandle, InternetOpenUrlW, InternetOpenW, InternetReadFile, HTTP_QUERY_FLAG_NUMBER,
        HTTP_QUERY_STATUS_CODE, INTERNET_FLAG_NO_CACHE_WRITE, INTERNET_FLAG_NO_COOKIES, INTERNET_FLAG_RELOAD,
        INTERNET_FLAG_SECURE, INTERNET_OPEN_TYPE_PRECONFIG,
    };

    /// Closes a WinINet handle when dropped.
    struct Handle(*mut core::ffi::c_void);
    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY: a handle WinINet gave us, closed once.
            unsafe {
                let _ = InternetCloseHandle(self.0);
            }
        }
    }

    /// GET `url` (https, redirects followed, no cache, no cookies) into `out`, at most `max_bytes`. `done` counts
    /// bytes for progress. Blocking: call from a background thread. Returns the number of bytes.
    pub fn fetch(url: &str, max_bytes: u64, out: &mut dyn Write, done: Option<&AtomicU64>) -> Result<u64, String> {
        // SAFETY: plain WinINet calls; every handle is closed by `Handle`.
        let session = unsafe { InternetOpenW(w!("Context Capsule"), INTERNET_OPEN_TYPE_PRECONFIG.0, None, None, 0) };
        if session.is_null() {
            return Err("no internet connection".into());
        }
        let session = Handle(session);
        let flags = INTERNET_FLAG_RELOAD | INTERNET_FLAG_NO_CACHE_WRITE | INTERNET_FLAG_NO_COOKIES | INTERNET_FLAG_SECURE;
        // SAFETY: as above.
        let request = unsafe { InternetOpenUrlW(session.0, &HSTRING::from(url), None, flags, None) };
        if request.is_null() {
            return Err("cannot reach the server".into());
        }
        let request = Handle(request);

        let mut status = 0u32;
        let mut len = std::mem::size_of::<u32>() as u32;
        // SAFETY: `status` is a u32 buffer of `len` bytes.
        let ok = unsafe {
            HttpQueryInfoW(
                request.0,
                HTTP_QUERY_STATUS_CODE | HTTP_QUERY_FLAG_NUMBER,
                Some(&mut status as *mut u32 as *mut core::ffi::c_void),
                &mut len,
                None,
            )
        };
        if ok.is_err() || status != 200 {
            return Err(format!("the server answered {status}"));
        }

        let mut buf = vec![0u8; 1 << 16];
        let mut total = 0u64;
        loop {
            let mut read = 0u32;
            // SAFETY: `buf` has `buf.len()` writable bytes.
            let ok = unsafe { InternetReadFile(request.0, buf.as_mut_ptr() as *mut core::ffi::c_void, buf.len() as u32, &mut read) };
            if ok.is_err() {
                return Err("the download was interrupted".into());
            }
            if read == 0 {
                break;
            }
            total += read as u64;
            if total > max_bytes {
                return Err("the download is larger than expected".into());
            }
            out.write_all(&buf[..read as usize]).map_err(|e| format!("cannot write the file: {e}"))?;
            if let Some(d) = done {
                d.store(total, Ordering::Relaxed);
            }
        }
        Ok(total)
    }

    /// The core's `Downloader` on WinINet.
    pub struct WinInet;

    impl context_capsule_core::update::Downloader for WinInet {
        fn get(&self, url: &str, max_bytes: u64) -> Result<Vec<u8>, String> {
            let mut body = Vec::new();
            fetch(url, max_bytes, &mut body, None)?;
            Ok(body)
        }

        fn save(&self, url: &str, dest: &Path, max_bytes: u64) -> Result<(), String> {
            let mut file = std::fs::File::create(dest).map_err(|e| format!("cannot write the file: {e}"))?;
            fetch(url, max_bytes, &mut file, None)?;
            file.sync_all().map_err(|e| format!("cannot write the file: {e}"))
        }
    }
}
