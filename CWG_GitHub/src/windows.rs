//! Windows-only fixed-host GitHub HTTPS. No production endpoint override.
use cwg_github::{GithubApi, MAX_EVENT_BYTES};
use cwg_security::Result;
use std::{ffi::c_void, ptr, time::Instant};
type Handle = *mut c_void;
const OPTION_CONNECT_RETRIES: u32 = 4;
const OPTION_DISABLE_FEATURE: u32 = 63;
const OPTION_SECURE_PROTOCOLS: u32 = 84;
const TLS_1_2: u32 = 0x800;
// Explicit per-session policy, independent of host defaults: TLS 1.2 and one
// connection attempt. No application retry or legacy-TLS fallback is added.
fn session_options() -> &'static [(u32, u32)] {
    &[
        (OPTION_SECURE_PROTOCOLS, TLS_1_2),
        (OPTION_CONNECT_RETRIES, 1),
    ]
}
fn close_status(closed: i32) -> Result<()> {
    if closed == 0 {
        Err("GITHUB_CLEANUP_UNKNOWN")
    } else {
        Ok(())
    }
}
fn combined_result<T>(operation: Result<T>, cleanup: Result<()>) -> Result<T> {
    match (operation, cleanup) {
        (operation, Ok(())) => operation,
        (Ok(_), Err(_)) => Err("GITHUB_CLEANUP_UNKNOWN"),
        (Err(error), Err(_)) => Err(match error {
            "GITHUB_TRANSPORT" => "GITHUB_TRANSPORT_AND_CLEANUP_UNKNOWN",
            "GITHUB_HANDLE" => "GITHUB_HANDLE_AND_CLEANUP_UNKNOWN",
            "GITHUB_STATUS_NOT_OK" => "GITHUB_STATUS_AND_CLEANUP_UNKNOWN",
            "GITHUB_DEADLINE" => "GITHUB_DEADLINE_AND_CLEANUP_UNKNOWN",
            "GITHUB_RESPONSE_TOO_LARGE" => "GITHUB_RESPONSE_SIZE_AND_CLEANUP_UNKNOWN",
            "GITHUB_RESPONSE_LENGTH_QUERY"
            | "GITHUB_RESPONSE_LENGTH_INVALID"
            | "GITHUB_RESPONSE_LENGTH_MISMATCH" => "GITHUB_RESPONSE_LENGTH_AND_CLEANUP_UNKNOWN",
            _ => "GITHUB_OPERATION_AND_CLEANUP_UNKNOWN",
        }),
    }
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetLastError() -> u32;
}
#[link(name = "winhttp")]
unsafe extern "system" {
    fn WinHttpOpen(
        agent: *const u16,
        access: u32,
        proxy: *const u16,
        bypass: *const u16,
        flags: u32,
    ) -> Handle;
    fn WinHttpConnect(session: Handle, server: *const u16, port: u16, reserved: u32) -> Handle;
    fn WinHttpOpenRequest(
        connection: Handle,
        verb: *const u16,
        object: *const u16,
        version: *const u16,
        referer: *const u16,
        accept: *const *const u16,
        flags: u32,
    ) -> Handle;
    fn WinHttpSetTimeouts(
        handle: Handle,
        resolve: i32,
        connect: i32,
        send: i32,
        receive: i32,
    ) -> i32;
    fn WinHttpSetOption(handle: Handle, option: u32, value: *mut c_void, length: u32) -> i32;
    fn WinHttpSendRequest(
        request: Handle,
        headers: *const u16,
        length: u32,
        optional: *const c_void,
        optional_length: u32,
        total: u32,
        context: usize,
    ) -> i32;
    fn WinHttpReceiveResponse(request: Handle, reserved: *mut c_void) -> i32;
    fn WinHttpQueryHeaders(
        request: Handle,
        level: u32,
        name: *const u16,
        buffer: *mut c_void,
        length: *mut u32,
        index: *mut u32,
    ) -> i32;
    fn WinHttpReadData(request: Handle, buffer: *mut c_void, length: u32, read: *mut u32) -> i32;
    fn WinHttpCloseHandle(handle: Handle) -> i32;
    #[cfg(test)]
    fn WinHttpQueryOption(handle: Handle, option: u32, value: *mut c_void, length: *mut u32)
    -> i32;
}
struct Owned(Handle);
impl Owned {
    fn new(handle: Handle) -> Result<Self> {
        if handle.is_null() {
            Err("GITHUB_HANDLE")
        } else {
            Ok(Self(handle))
        }
    }
    fn close(mut self) -> CloseOutcome {
        let handle = std::mem::replace(&mut self.0, ptr::null_mut());
        // SAFETY: exactly this successfully created, synchronous owned handle;
        // disarm before closing so an uncertain close is never blindly retried.
        let closed = unsafe { WinHttpCloseHandle(handle) };
        CloseOutcome {
            result: close_status(closed),
            #[cfg(test)]
            succeeded: closed != 0,
        }
    }
}
impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_null() {
            let handle = std::mem::replace(&mut self.0, ptr::null_mut());
            // SAFETY: panic/unexpected-path fallback for an owned handle. Normal
            // success and error returns use explicit checked close_all instead.
            let _ = unsafe { WinHttpCloseHandle(handle) };
        }
    }
}
struct CloseOutcome {
    result: Result<()>,
    #[cfg(test)]
    succeeded: bool,
}
#[cfg(test)]
#[derive(Default, Debug)]
struct CloseStats {
    opened: usize,
    closed: usize,
    unknown: usize,
}
#[derive(Default)]
struct Handles {
    values: Vec<Owned>,
    #[cfg(test)]
    stats: CloseStats,
}
impl Handles {
    fn add(&mut self, raw: Handle) -> Result<Handle> {
        self.values.push(Owned::new(raw)?);
        #[cfg(test)]
        {
            self.stats.opened += 1;
        }
        Ok(raw)
    }
    fn close_all(mut self) -> CleanupOutcome {
        let mut result = Ok(());
        while let Some(handle) = self.values.pop() {
            let closed = handle.close();
            #[cfg(test)]
            {
                if closed.succeeded {
                    self.stats.closed += 1;
                } else {
                    self.stats.unknown += 1;
                }
            }
            if closed.result.is_err() {
                result = Err("GITHUB_CLEANUP_UNKNOWN");
            }
        }
        CleanupOutcome {
            result,
            #[cfg(test)]
            stats: std::mem::take(&mut self.stats),
        }
    }
}
impl Drop for Handles {
    fn drop(&mut self) {
        while let Some(handle) = self.values.pop() {
            drop(handle);
        }
    }
}
struct CleanupOutcome {
    result: Result<()>,
    #[cfg(test)]
    stats: CloseStats,
}
struct RequestOutcome {
    result: Result<Vec<u8>>,
    #[cfg(test)]
    stats: CloseStats,
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn ok(v: i32) -> Result<()> {
    if v == 0 {
        Err("GITHUB_TRANSPORT")
    } else {
        Ok(())
    }
}
fn remaining(deadline: Instant) -> Result<i32> {
    let ms = deadline
        .saturating_duration_since(Instant::now())
        .as_millis();
    if ms == 0 {
        Err("GITHUB_DEADLINE")
    } else {
        Ok(ms.min(1500) as i32)
    }
}
fn validate_path(path: &str) -> Result<()> {
    if !path.starts_with("/repos/")
        || path.len() > 512
        || path.split('/').any(|p| p == "." || p == "..")
        || !path
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"/-_.".contains(&c))
    {
        return Err("GITHUB_PATH_REFUSED");
    }
    Ok(())
}
fn response_length(succeeded: i32, size: u32, length: u32, error: u32) -> Result<Option<usize>> {
    if succeeded == 0 {
        // ERROR_WINHTTP_HEADER_NOT_FOUND. All other query failures are refusal.
        return if error == 12150 {
            Ok(None)
        } else {
            Err("GITHUB_RESPONSE_LENGTH_QUERY")
        };
    }
    if size != 4 {
        return Err("GITHUB_RESPONSE_LENGTH_INVALID");
    }
    if length as usize > MAX_EVENT_BYTES {
        return Err("GITHUB_RESPONSE_TOO_LARGE");
    }
    Ok(Some(length as usize))
}
struct Endpoint<'a> {
    host: &'a str,
    port: u16,
    secure: bool,
}
fn request(
    path: &str,
    deadline: Instant,
    endpoint: Endpoint<'_>,
    header_text: &str,
) -> RequestOutcome {
    let mut handles = Handles::default();
    let result = (|| {
        validate_path(path)?;
        let timeout = remaining(deadline)?;
        let agent = wide("Chat-Worker-GitHub/0.1");
        let host = wide(endpoint.host);
        let verb = wide("GET");
        let object = wide(path);
        let headers = wide(header_text);
        // SAFETY: synchronous WinHTTP and live UTF-16/bounded data buffers.
        // Handles are closed explicitly child-before-parent on every return.
        unsafe {
            let session =
                handles.add(WinHttpOpen(agent.as_ptr(), 1, ptr::null(), ptr::null(), 0))?;
            ok(WinHttpSetTimeouts(
                session, timeout, timeout, timeout, timeout,
            ))?;
            for &(option, mut value) in session_options() {
                ok(WinHttpSetOption(
                    session,
                    option,
                    (&mut value as *mut u32).cast(),
                    4,
                ))?;
            }
            let connection =
                handles.add(WinHttpConnect(session, host.as_ptr(), endpoint.port, 0))?;
            let request = handles.add(WinHttpOpenRequest(
                connection,
                verb.as_ptr(),
                object.as_ptr(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
                if endpoint.secure { 0x0080_0000 } else { 0 },
            ))?;
            // Request-only: cookies, redirects and automatic authentication off.
            let mut disabled = 7u32;
            ok(WinHttpSetOption(
                request,
                OPTION_DISABLE_FEATURE,
                (&mut disabled as *mut u32).cast(),
                4,
            ))?;
            remaining(deadline)?;
            ok(WinHttpSendRequest(
                request,
                headers.as_ptr(),
                (headers.len() - 1) as u32,
                ptr::null(),
                0,
                0,
                0,
            ))?;
            remaining(deadline)?;
            ok(WinHttpReceiveResponse(request, ptr::null_mut()))?;
            let mut status = 0u32;
            let mut size = 4u32;
            ok(WinHttpQueryHeaders(
                request,
                19 | 0x2000_0000,
                ptr::null(),
                (&mut status as *mut u32).cast(),
                &mut size,
                ptr::null_mut(),
            ))?;
            if status != 200 {
                return Err("GITHUB_STATUS_NOT_OK");
            }
            let mut length = 0u32;
            let mut size = 4u32;
            let queried = WinHttpQueryHeaders(
                request,
                5 | 0x2000_0000,
                ptr::null(),
                (&mut length as *mut u32).cast(),
                &mut size,
                ptr::null_mut(),
            );
            let error = if queried == 0 { GetLastError() } else { 0 };
            let expected = response_length(queried, size, length, error)?;
            let mut result = Vec::new();
            let mut chunk = [0u8; 8192];
            loop {
                let timeout = remaining(deadline)?;
                ok(WinHttpSetTimeouts(
                    request, timeout, timeout, timeout, timeout,
                ))?;
                let mut n = 0u32;
                ok(WinHttpReadData(
                    request,
                    chunk.as_mut_ptr().cast(),
                    chunk.len() as u32,
                    &mut n,
                ))?;
                remaining(deadline)?;
                if n == 0 {
                    if expected.is_some_and(|length| result.len() != length) {
                        return Err("GITHUB_RESPONSE_LENGTH_MISMATCH");
                    }
                    break;
                }
                if n as usize > chunk.len() || result.len() + n as usize > MAX_EVENT_BYTES {
                    return Err("GITHUB_RESPONSE_TOO_LARGE");
                }
                if expected.is_some_and(|length| result.len() + n as usize > length) {
                    return Err("GITHUB_RESPONSE_LENGTH_MISMATCH");
                }
                result.extend_from_slice(&chunk[..n as usize]);
            }
            Ok(result)
        }
    })();
    let cleanup = handles.close_all();
    RequestOutcome {
        result: combined_result(result, cleanup.result),
        #[cfg(test)]
        stats: cleanup.stats,
    }
}
pub struct Client<'a> {
    token: &'a str,
    deadline: Instant,
}
impl<'a> Client<'a> {
    pub fn new(token: &'a str, deadline: Instant) -> Result<Self> {
        if token.len() < 16 || token.len() > 4096 || !token.bytes().all(|c| (33..=126).contains(&c))
        {
            return Err("GITHUB_TOKEN_FORMAT");
        }
        Ok(Self { token, deadline })
    }
}
impl GithubApi for Client<'_> {
    fn get(&self, path: &str) -> Result<Vec<u8>> {
        // The only production endpoint; payload/config/environment cannot alter it.
        let headers = format!(
            "Authorization: Bearer {}\r\nAccept: application/vnd.github+json\r\nX-GitHub-Api-Version: 2022-11-28\r\n",
            self.token
        );
        request(
            path,
            self.deadline,
            Endpoint {
                host: "api.github.com",
                port: 443,
                secure: true,
            },
            &headers,
        )
        .result
    }
}
/// Controlled local integration only; compiled out of every production build.
#[cfg(test)]
pub(crate) struct FixtureClient {
    address: std::net::SocketAddr,
    deadline: Instant,
}
#[cfg(test)]
impl FixtureClient {
    pub(crate) fn new(address: std::net::SocketAddr, deadline: Instant) -> Result<Self> {
        if !address.ip().is_loopback() || address.port() == 0 {
            return Err("GITHUB_FIXTURE_LOOPBACK_REQUIRED");
        }
        Ok(Self { address, deadline })
    }
}
#[cfg(test)]
impl GithubApi for FixtureClient {
    fn get(&self, path: &str) -> Result<Vec<u8>> {
        request(
            path,
            self.deadline,
            Endpoint {
                host: &self.address.ip().to_string(),
                port: self.address.port(),
                secure: false,
            },
            "Accept: application/json\r\n",
        )
        .result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::{SocketAddr, TcpListener},
        sync::mpsc,
        thread,
        time::Duration,
    };
    // Private and test-only: exact loopback endpoint, no token/Authorization,
    // and HTTP fixture transport. This does not qualify production TLS/GitHub.
    fn fixture_get(address: SocketAddr, deadline: Instant) -> RequestOutcome {
        assert!(address.ip().is_loopback());
        request(
            "/repos/fixture/repo",
            deadline,
            Endpoint {
                host: &address.ip().to_string(),
                port: address.port(),
                secure: false,
            },
            "Accept: application/json\r\n",
        )
    }
    fn server(
        response: Vec<u8>,
        delay: Duration,
    ) -> (SocketAddr, mpsc::Receiver<Vec<u8>>, thread::JoinHandle<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let (send, receive) = mpsc::channel();
        let worker = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(4);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(_) => return,
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut raw = Vec::new();
            let mut byte = [0u8; 1];
            while raw.len() < 16_384 && !raw.ends_with(b"\r\n\r\n") {
                if stream.read(&mut byte).unwrap_or(0) == 0 {
                    break;
                }
                raw.push(byte[0]);
            }
            let _ = send.send(raw);
            thread::sleep(delay);
            let _ = stream.write_all(&response);
        });
        (address, receive, worker)
    }
    fn response(status: &str, body: &[u8], extra: &str) -> Vec<u8> {
        let mut response = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n",
            body.len()
        )
        .into_bytes();
        response.extend_from_slice(body);
        response
    }
    fn closed(outcome: &RequestOutcome, expected: usize) {
        assert_eq!(outcome.stats.opened, expected);
        assert_eq!(outcome.stats.closed, expected);
        assert_eq!(outcome.stats.unknown, 0);
    }
    #[test]
    fn audit_explicit_modern_tls_policy_is_required() {
        let mask = session_options()
            .iter()
            .find(|(key, _)| *key == OPTION_SECURE_PROTOCOLS)
            .map(|(_, value)| *value)
            .expect("explicit TLS protocol policy required");
        assert_ne!(mask & TLS_1_2, 0);
        assert_eq!(mask & (0x8 | 0x20 | 0x80 | 0x200), 0);
    }
    #[test]
    fn audit_connect_attempt_policy_is_single_attempt() {
        let count = session_options()
            .iter()
            .find(|(key, _)| *key == OPTION_CONNECT_RETRIES)
            .map(|(_, value)| *value)
            .expect("explicit connection-attempt limit required");
        assert_eq!(count, 1);
    }
    #[test]
    fn audit_failed_native_close_is_not_reported_as_success() {
        assert_eq!(close_status(0), Err("GITHUB_CLEANUP_UNKNOWN"));
        assert_eq!(close_status(1), Ok(()));
    }
    #[test]
    fn audit_cleanup_failure_keeps_operation_failure_class() {
        assert_eq!(
            combined_result::<()>(Err("GITHUB_TRANSPORT"), Err("GITHUB_CLEANUP_UNKNOWN")),
            Err("GITHUB_TRANSPORT_AND_CLEANUP_UNKNOWN")
        );
        assert_eq!(
            combined_result::<()>(Err("GITHUB_STATUS_NOT_OK"), Err("GITHUB_CLEANUP_UNKNOWN")),
            Err("GITHUB_STATUS_AND_CLEANUP_UNKNOWN")
        );
        assert_eq!(
            combined_result::<()>(Ok(()), Err("GITHUB_CLEANUP_UNKNOWN")),
            Err("GITHUB_CLEANUP_UNKNOWN")
        );
        assert_eq!(
            combined_result::<()>(Err("GITHUB_TRANSPORT"), Ok(())),
            Err("GITHUB_TRANSPORT")
        );
    }
    #[test]
    fn audit_response_length_query_errors_are_not_treated_as_absent() {
        assert_eq!(response_length(0, 4, 0, 12150), Ok(None));
        for error in [0, 13, 122, 12002] {
            assert_eq!(
                response_length(0, 4, 0, error),
                Err("GITHUB_RESPONSE_LENGTH_QUERY")
            );
        }
        assert_eq!(
            response_length(1, 8, 2, 0),
            Err("GITHUB_RESPONSE_LENGTH_INVALID")
        );
        assert_eq!(
            response_length(1, 4, MAX_EVENT_BYTES as u32 + 1, 0),
            Err("GITHUB_RESPONSE_TOO_LARGE")
        );
        assert_eq!(
            response_length(1, 4, MAX_EVENT_BYTES as u32, 0),
            Ok(Some(MAX_EVENT_BYTES))
        );
    }
    #[test]
    fn audit_native_session_policy_is_observable() {
        let agent = wide("CWG-native-test");
        let mut handles = Handles::default();
        // SAFETY: owned synchronous session, u32 option buffers, no networking.
        unsafe {
            let session = handles
                .add(WinHttpOpen(agent.as_ptr(), 1, ptr::null(), ptr::null(), 0))
                .unwrap();
            for &(option, mut value) in session_options() {
                ok(WinHttpSetOption(
                    session,
                    option,
                    (&mut value as *mut u32).cast(),
                    4,
                ))
                .unwrap();
            }
            let mut retries = 0u32;
            let mut size = 4;
            ok(WinHttpQueryOption(
                session,
                OPTION_CONNECT_RETRIES,
                (&mut retries as *mut u32).cast(),
                &mut size,
            ))
            .unwrap();
            assert_eq!(retries, 1);
            let mut protocols = 0u32;
            let mut size = 4;
            // SECURE_PROTOCOLS is set-only in the documented WinHTTP option
            // matrix. The native SetOption calls above must succeed; a query
            // is not evidence of the negotiated production TLS protocol.
            assert_eq!(
                WinHttpQueryOption(
                    session,
                    OPTION_SECURE_PROTOCOLS,
                    (&mut protocols as *mut u32).cast(),
                    &mut size,
                ),
                0
            );
            assert_eq!(GetLastError(), 12009); // ERROR_WINHTTP_INVALID_OPTION
            let requested = session_options()
                .iter()
                .find(|(option, _)| *option == OPTION_SECURE_PROTOCOLS)
                .unwrap()
                .1;
            assert_ne!(requested & TLS_1_2, 0);
            assert_eq!(requested & (0x8 | 0x20 | 0x80 | 0x200), 0);
        }
        let cleanup = handles.close_all();
        cleanup.result.unwrap();
        assert_eq!(cleanup.stats.closed, 1);
    }
    #[test]
    fn audit_native_loopback_get_has_no_credentials_and_closes_owned_handles() {
        let (address, capture, worker) = server(
            response("200 OK", b"{\"fixture\":true}", ""),
            Duration::ZERO,
        );
        let outcome = fixture_get(address, Instant::now() + Duration::from_secs(2));
        worker.join().unwrap();
        assert_eq!(outcome.result.as_ref().unwrap(), b"{\"fixture\":true}");
        closed(&outcome, 3);
        let headers = String::from_utf8(capture.recv().unwrap())
            .unwrap()
            .to_ascii_lowercase();
        assert!(headers.starts_with("get /repos/fixture/repo http/1.1"));
        assert!(!headers.contains("authorization:"));
        assert!(!headers.contains("cookie:"));
    }
    #[test]
    fn audit_native_redirect_is_refused_without_following_location() {
        let trap = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        trap.set_nonblocking(true).unwrap();
        let location = format!(
            "Location: http://{}/repos/fixture/trap\r\n",
            trap.local_addr().unwrap()
        );
        let (address, _, worker) = server(response("302 Found", b"", &location), Duration::ZERO);
        let outcome = fixture_get(address, Instant::now() + Duration::from_secs(2));
        worker.join().unwrap();
        assert_eq!(outcome.result, Err("GITHUB_STATUS_NOT_OK"));
        closed(&outcome, 3);
        assert!(
            matches!(trap.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
        );
    }
    #[test]
    fn audit_native_denied_status_and_auth_challenge_do_not_authorize() {
        for status in [
            "401 Unauthorized",
            "403 Forbidden",
            "404 Not Found",
            "429 Too Many Requests",
            "500 Server Error",
            "503 Service Unavailable",
        ] {
            let (address, capture, worker) = server(
                response(status, b"", "WWW-Authenticate: Negotiate\r\n"),
                Duration::ZERO,
            );
            let outcome = fixture_get(address, Instant::now() + Duration::from_secs(2));
            worker.join().unwrap();
            assert_eq!(outcome.result, Err("GITHUB_STATUS_NOT_OK"));
            closed(&outcome, 3);
            assert!(
                !String::from_utf8(capture.recv().unwrap())
                    .unwrap()
                    .to_ascii_lowercase()
                    .contains("authorization:")
            );
        }
    }
    #[test]
    fn audit_native_response_limit_fails_closed_and_closes_handles() {
        let (address, _, worker) = server(
            response("200 OK", &vec![b'x'; MAX_EVENT_BYTES + 1], ""),
            Duration::ZERO,
        );
        let outcome = fixture_get(address, Instant::now() + Duration::from_secs(2));
        worker.join().unwrap();
        assert_eq!(outcome.result, Err("GITHUB_RESPONSE_TOO_LARGE"));
        closed(&outcome, 3);
    }
    #[test]
    fn audit_native_truncated_response_is_not_accepted() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\n{}".to_vec();
        let (address, _, worker) = server(raw, Duration::ZERO);
        let outcome = fixture_get(address, Instant::now() + Duration::from_secs(2));
        worker.join().unwrap();
        assert_eq!(outcome.result, Err("GITHUB_RESPONSE_LENGTH_MISMATCH"));
        closed(&outcome, 3);
    }
    #[test]
    fn audit_native_exact_response_boundary_and_missing_length() {
        let body = vec![b'x'; MAX_EVENT_BYTES];
        let (address, _, worker) = server(response("200 OK", &body, ""), Duration::ZERO);
        let outcome = fixture_get(address, Instant::now() + Duration::from_secs(2));
        worker.join().unwrap();
        assert_eq!(outcome.result.as_ref().unwrap().len(), MAX_EVENT_BYTES);
        closed(&outcome, 3);
        let (address, _, worker) = server(
            b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{}".to_vec(),
            Duration::ZERO,
        );
        let outcome = fixture_get(address, Instant::now() + Duration::from_secs(2));
        worker.join().unwrap();
        assert_eq!(outcome.result.as_ref().unwrap().as_slice(), b"{}");
        closed(&outcome, 3);
    }
    #[test]
    fn audit_fixture_client_refuses_non_loopback_without_network() {
        let deadline = Instant::now() + Duration::from_secs(1);
        assert_eq!(
            FixtureClient::new("192.0.2.1:80".parse().unwrap(), deadline).err(),
            Some("GITHUB_FIXTURE_LOOPBACK_REQUIRED")
        );
        assert_eq!(
            FixtureClient::new("127.0.0.1:0".parse().unwrap(), deadline).err(),
            Some("GITHUB_FIXTURE_LOOPBACK_REQUIRED")
        );
    }
    #[test]
    fn audit_token_format_rejects_header_injection_before_network() {
        for token in [
            String::new(),
            "x".repeat(15),
            "x".repeat(4097),
            "x xxxxxxxxxxxxxxxx".into(),
            "xxxxxxxxxxxxxxxx\r\nInjected: true".into(),
        ] {
            assert_eq!(
                Client::new(&token, Instant::now()).err(),
                Some("GITHUB_TOKEN_FORMAT")
            );
        }
    }
    #[test]
    fn audit_native_timeout_refuses_and_releases_owned_handles() {
        let (address, _, worker) =
            server(response("200 OK", b"{}", ""), Duration::from_millis(250));
        let outcome = fixture_get(address, Instant::now() + Duration::from_millis(80));
        worker.join().unwrap();
        assert!(matches!(
            outcome.result,
            Err("GITHUB_DEADLINE" | "GITHUB_TRANSPORT")
        ));
        closed(&outcome, 3);
    }
    #[test]
    fn audit_invalid_path_or_deadline_opens_no_handle() {
        for path in [
            "/repos/../evil",
            "https://attacker.invalid",
            "/repos/a/b?x=1",
            "/repos/a/b\r\nInjected",
        ] {
            let outcome = request(
                path,
                Instant::now() + Duration::from_secs(1),
                Endpoint {
                    host: "unused.invalid",
                    port: 443,
                    secure: true,
                },
                "",
            );
            assert_eq!(outcome.result, Err("GITHUB_PATH_REFUSED"));
            closed(&outcome, 0);
        }
        let outcome = request(
            "/repos/a/b",
            Instant::now() - Duration::from_millis(1),
            Endpoint {
                host: "unused.invalid",
                port: 443,
                secure: true,
            },
            "",
        );
        assert_eq!(outcome.result, Err("GITHUB_DEADLINE"));
        closed(&outcome, 0);
    }
}
