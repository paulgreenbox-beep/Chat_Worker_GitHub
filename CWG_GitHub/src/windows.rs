//! Windows-only fixed-host GitHub HTTPS. No shell, redirect, cookie, or retry.
use cwg_github::{GithubApi, MAX_EVENT_BYTES};
use cwg_security::Result;
use std::{ffi::c_void, ptr, time::Instant};
type Handle = *mut c_void;
#[link(name = "winhttp")]
unsafe extern "system" {
    fn WinHttpOpen(agent: *const u16, access: u32, proxy: *const u16, bypass: *const u16, flags: u32) -> Handle;
    fn WinHttpConnect(session: Handle, server: *const u16, port: u16, reserved: u32) -> Handle;
    fn WinHttpOpenRequest(connection: Handle, verb: *const u16, object: *const u16, version: *const u16, referer: *const u16, accept: *const *const u16, flags: u32) -> Handle;
    fn WinHttpSetTimeouts(handle: Handle, resolve: i32, connect: i32, send: i32, receive: i32) -> i32;
    fn WinHttpSetOption(handle: Handle, option: u32, value: *mut c_void, length: u32) -> i32;
    fn WinHttpSendRequest(request: Handle, headers: *const u16, length: u32, optional: *const c_void, optional_length: u32, total: u32, context: usize) -> i32;
    fn WinHttpReceiveResponse(request: Handle, reserved: *mut c_void) -> i32;
    fn WinHttpQueryHeaders(request: Handle, level: u32, name: *const u16, buffer: *mut c_void, length: *mut u32, index: *mut u32) -> i32;
    fn WinHttpReadData(request: Handle, buffer: *mut c_void, length: u32, read: *mut u32) -> i32;
    fn WinHttpCloseHandle(handle: Handle) -> i32;
}
struct Owned(Handle);
impl Owned {
    fn new(handle: Handle) -> Result<Self> { if handle.is_null() { Err("GITHUB_HANDLE") } else { Ok(Self(handle)) } }
}
impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: Owns exactly this successfully opened synchronous handle.
        unsafe { WinHttpCloseHandle(self.0); }
    }
}
fn wide(s: &str) -> Vec<u16> { s.encode_utf16().chain(Some(0)).collect() }
fn ok(v: i32) -> Result<()> { if v == 0 { Err("GITHUB_TRANSPORT") } else { Ok(()) } }

pub struct Client<'a> { token: &'a str, deadline: Instant }
impl<'a> Client<'a> {
    pub fn new(token: &'a str, deadline: Instant) -> Result<Self> {
        if token.len() < 16 || token.len() > 4096 || !token.bytes().all(|c| (33..=126).contains(&c)) { return Err("GITHUB_TOKEN_FORMAT"); }
        Ok(Self { token, deadline })
    }
    fn remaining(&self) -> Result<i32> {
        let ms = self.deadline.saturating_duration_since(Instant::now()).as_millis();
        if ms == 0 { Err("GITHUB_DEADLINE") } else { Ok(ms.min(1500) as i32) }
    }
}
impl GithubApi for Client<'_> {
    fn get(&self, path: &str) -> Result<Vec<u8>> {
        if !path.starts_with("/repos/") || path.len() > 512 || path.split('/').any(|p| p == "." || p == "..")
            || !path.bytes().all(|c| c.is_ascii_alphanumeric() || b"/-_.".contains(&c)) {
            return Err("GITHUB_PATH_REFUSED");
        }
        let timeout = self.remaining()?;
        let agent = wide("Chat-Worker-GitHub/0.1"); let host = wide("api.github.com");
        let verb = wide("GET"); let object = wide(path);
        let headers = wide(&format!("Authorization: Bearer {}\r\nAccept: application/vnd.github+json\r\nX-GitHub-Api-Version: 2022-11-28\r\n", self.token));
        // SAFETY: synchronous WinHTTP; every pointer references a live UTF-16
        // or fixed-size buffer; handles are RAII-owned in child-before-parent
        // drop order. No certificate-validation bypass flags are used.
        unsafe {
            // WINHTTP_ACCESS_TYPE_NO_PROXY = 1; no WPAD/proxy credential discovery.
            let session = Owned::new(WinHttpOpen(agent.as_ptr(), 1, ptr::null(), ptr::null(), 0))?;
            ok(WinHttpSetTimeouts(session.0, timeout, timeout, timeout, timeout))?;
            let connection = Owned::new(WinHttpConnect(session.0, host.as_ptr(), 443, 0))?;
            let request = Owned::new(WinHttpOpenRequest(connection.0, verb.as_ptr(), object.as_ptr(), ptr::null(), ptr::null(), ptr::null(), 0x0080_0000))?;
            // WINHTTP_OPTION_DISABLE_FEATURE = 63: cookies (1), redirects (2),
            // automatic authentication (4). Explicit bearer header remains.
            let mut disabled = 7u32;
            ok(WinHttpSetOption(request.0, 63, (&mut disabled as *mut u32).cast(), 4))?;
            self.remaining()?;
            ok(WinHttpSendRequest(request.0, headers.as_ptr(), (headers.len() - 1) as u32, ptr::null(), 0, 0, 0))?;
            self.remaining()?;
            ok(WinHttpReceiveResponse(request.0, ptr::null_mut()))?;
            let mut status = 0u32; let mut size = 4u32;
            // WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER.
            ok(WinHttpQueryHeaders(request.0, 19 | 0x2000_0000, ptr::null(), (&mut status as *mut u32).cast(), &mut size, ptr::null_mut()))?;
            if status != 200 { return Err("GITHUB_STATUS_NOT_OK"); }
            let mut result = Vec::new(); let mut chunk = [0u8; 8192];
            loop {
                let remaining = self.remaining()?;
                ok(WinHttpSetTimeouts(request.0, remaining, remaining, remaining, remaining))?;
                let mut n = 0u32;
                ok(WinHttpReadData(request.0, chunk.as_mut_ptr().cast(), chunk.len() as u32, &mut n))?;
                if n == 0 { break; }
                if n as usize > chunk.len() || result.len() + n as usize > MAX_EVENT_BYTES { return Err("GITHUB_RESPONSE_TOO_LARGE"); }
                result.extend_from_slice(&chunk[..n as usize]);
            }
            Ok(result)
        }
    }
}
