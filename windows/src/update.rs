use std::ffi::c_void;
use std::os::windows::process::CommandExt;

use windows::Win32::Networking::WinHttp::{
    WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE, WinHttpCloseHandle,
    WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest,
    WinHttpSetTimeouts,
};
use windows::core::{PCWSTR, w};

use snowdrift_core::version::{field, newer};

const REPOSITORY: &str = "LerkOFF/snowdrift";
const INSTALLER: &str = "Snowdrift.exe";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// A release newer than this build that carries an installer for Windows.
pub struct Release {
    pub version: String,
}

/// Asks GitHub for the latest release. Releases with nothing for Windows don't count as updates.
pub fn check() -> Result<Option<Release>, String> {
    let body = String::from_utf8_lossy(&fetch("api.github.com", &format!("/repos/{REPOSITORY}/releases/latest"))?).into_owned();
    let tag = field(&body, "tag_name").ok_or("the latest release has no tag")?;
    let version = tag.trim_start_matches('v');
    if !body.contains(&format!("\"{INSTALLER}\"")) || !newer(version, env!("CARGO_PKG_VERSION")) {
        return Ok(None);
    }
    Ok(Some(Release { version: version.to_string() }))
}

/// Downloads the installer of `release` and starts it: it closes this copy, replaces it and starts the new one.
pub fn install(release: &Release) -> Result<(), String> {
    let bytes = fetch("github.com", &format!("/{REPOSITORY}/releases/download/v{}/{INSTALLER}", release.version))?;
    if bytes.len() < 100_000 || !bytes.starts_with(b"MZ") {
        return Err("the download isn't a Windows program".to_string());
    }
    let folder = std::env::temp_dir().join("Snowdrift-update");
    std::fs::create_dir_all(&folder).map_err(|error| format!("couldn't create {}: {error}", folder.display()))?;
    let exe = folder.join(INSTALLER);
    std::fs::write(&exe, bytes).map_err(|error| format!("couldn't write {}: {error}", exe.display()))?;
    std::process::Command::new(&exe)
        .arg("--install")
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|error| format!("couldn't start the installer: {error}"))?;
    Ok(())
}

struct Handle(*mut c_void);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

fn fetch(host: &str, path: &str) -> Result<Vec<u8>, String> {
    let failed = |what: &str| format!("{what} failed: {}", windows::core::Error::from_thread().message());
    let path: Vec<u16> = path.encode_utf16().chain([0]).collect();
    let host: Vec<u16> = host.encode_utf16().chain([0]).collect();
    unsafe {
        let session = Handle(WinHttpOpen(w!("Snowdrift"), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PCWSTR::null(), PCWSTR::null(), 0));
        if session.0.is_null() {
            return Err(failed("Opening a connection"));
        }
        let _ = WinHttpSetTimeouts(session.0, 10_000, 10_000, 10_000, 60_000);
        let connection = Handle(WinHttpConnect(session.0, PCWSTR(host.as_ptr()), 443, 0));
        if connection.0.is_null() {
            return Err(failed("Connecting to GitHub"));
        }
        let request = Handle(WinHttpOpenRequest(
            connection.0,
            w!("GET"),
            PCWSTR(path.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        ));
        if request.0.is_null() {
            return Err(failed("Preparing the request"));
        }
        let headers: Vec<u16> = "Accept: application/vnd.github+json\r\n".encode_utf16().collect();
        WinHttpSendRequest(request.0, Some(&headers), None, 0, 0, 0).map_err(|error| error.message())?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut()).map_err(|error| error.message())?;

        let mut status = 0u32;
        let mut size = 4u32;
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&raw mut status).cast()),
            &mut size,
            std::ptr::null_mut(),
        )
        .map_err(|error| error.message())?;
        if status != 200 {
            return Err(format!("GitHub answered {status}"));
        }

        let mut body = Vec::new();
        let mut buffer = vec![0u8; 16 * 1024];
        loop {
            let mut read = 0u32;
            WinHttpReadData(request.0, buffer.as_mut_ptr().cast(), buffer.len() as u32, &mut read).map_err(|error| error.message())?;
            if read == 0 {
                break;
            }
            body.extend_from_slice(&buffer[..read as usize]);
        }
        Ok(body)
    }
}
