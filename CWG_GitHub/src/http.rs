//! Deliberately narrow loopback HTTP endpoint: one Content-Length POST and close.
//! HTTPS termination/public exposure belongs to an independently managed proxy.
use crate::MAX_EVENT_BYTES;
use cwg_security::Result;
use std::{collections::BTreeMap, io::Read};
const MAX_HEADERS: usize = 16 * 1024;
pub struct Request { pub event: String, pub delivery: String, pub signature: String, pub body: Vec<u8> }
pub fn read_request(reader: &mut impl Read) -> Result<Request> {
    let mut header = Vec::new(); let mut byte = [0; 1];
    while !header.ends_with(b"\r\n\r\n") {
        if header.len() >= MAX_HEADERS { return Err("HEADERS_TOO_LARGE"); }
        reader.read_exact(&mut byte).map_err(|_| "HTTP_TRUNCATED_OR_TIMEOUT")?;
        header.push(byte[0]);
    }
    let text = std::str::from_utf8(&header).map_err(|_| "HTTP_HEADERS_INVALID")?;
    let mut lines = text[..text.len() - 4].split("\r\n");
    if lines.next() != Some("POST /github HTTP/1.1") { return Err("HTTP_ROUTE_UNSUPPORTED"); }
    let mut headers = BTreeMap::new();
    for line in lines {
        let (name, value) = line.split_once(':').ok_or("HTTP_HEADERS_INVALID")?;
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            || !value.bytes().all(|b| (32..=126).contains(&b)) {
            return Err("HTTP_HEADERS_INVALID");
        }
        if headers.insert(name.to_ascii_lowercase(), value.trim().to_owned()).is_some() { return Err("HTTP_DUPLICATE_HEADER"); }
    }
    if headers.contains_key("transfer-encoding") || headers.contains_key("content-encoding") || headers.contains_key("expect")
        || !headers.contains_key("host") || headers.get("content-type").map(|v| v.split(';').next().unwrap_or("").trim()) != Some("application/json") {
        return Err("HTTP_FRAMING_UNSUPPORTED");
    }
    let field = |name: &str| headers.get(name).cloned().ok_or("HTTP_HEADER_MISSING");
    let length = field("content-length")?;
    if length.is_empty() || !length.bytes().all(|b| b.is_ascii_digit()) { return Err("HTTP_LENGTH_INVALID"); }
    let length: usize = length.parse().map_err(|_| "HTTP_LENGTH_INVALID")?;
    if length == 0 || length > MAX_EVENT_BYTES { return Err("HTTP_BODY_SIZE"); }
    let mut body = vec![0; length]; reader.read_exact(&mut body).map_err(|_| "HTTP_TRUNCATED_OR_TIMEOUT")?;
    Ok(Request { event: field("x-github-event")?, delivery: field("x-github-delivery")?, signature: field("x-hub-signature-256")?, body })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn raw(extra: &str, length: &str, body: &str) -> Vec<u8> {
        format!("POST /github HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nX-GitHub-Event: issue_comment\r\nX-GitHub-Delivery: test\r\nX-Hub-Signature-256: sha256=test\r\nContent-Length: {length}\r\n{extra}\r\n{body}").into_bytes()
    }
    #[test] fn preserves_exact_body() { assert_eq!(read_request(&mut &raw("", "2", "{}")[..]).unwrap().body, b"{}"); }
    #[test] fn duplicate_length_and_signature_are_refused() {
        for extra in ["content-length: 2\r\n", "X-Hub-Signature-256: other\r\n"] { assert!(read_request(&mut &raw(extra, "2", "{}")[..]).is_err()); }
    }
    #[test] fn ambiguous_framing_refused() {
        for extra in ["Transfer-Encoding: chunked\r\n", "Content-Encoding: gzip\r\n", "Expect: 100-continue\r\n"] { assert!(read_request(&mut &raw(extra, "2", "{}")[..]).is_err()); }
    }
    #[test] fn length_bounds_and_truncation_refused() {
        for len in ["0", "+2", "-2", "999999999999999999999", "3"] { assert!(read_request(&mut &raw("", len, "{}")[..]).is_err()); }
    }
    #[test] fn header_newline_injection_refused() { assert!(read_request(&mut &raw("Bad: value\nInjected: yes\r\n", "2", "{}")[..]).is_err()); }

    struct HeadersOnly { bytes: Vec<u8>, position: usize, body_attempted: bool }
    impl HeadersOnly {
        fn new(mut bytes: Vec<u8>) -> Self {
            let end = bytes.windows(4).position(|part| part == b"\r\n\r\n").unwrap() + 4;
            bytes.truncate(end);
            Self { bytes, position: 0, body_attempted: false }
        }
    }
    impl Read for HeadersOnly {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            if self.position == self.bytes.len() {
                self.body_attempted = true;
                return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "fixture forbids body read"));
            }
            let count = out.len().min(self.bytes.len() - self.position);
            out[..count].copy_from_slice(&self.bytes[self.position..self.position + count]);
            self.position += count;
            Ok(count)
        }
    }
    #[test]
    fn http_security_regression_empty_host_is_rejected() {
        let bytes = String::from_utf8(raw("", "2", "{}")).unwrap().replace("Host: localhost", "Host:");
        assert!(read_request(&mut bytes.as_bytes()).is_err());
    }
    #[test]
    fn http_security_regression_missing_signature_is_refused_before_body_read() {
        let bytes = String::from_utf8(raw("", "2", "{}")).unwrap().replace("X-Hub-Signature-256: sha256=test\r\n", "");
        let mut reader = HeadersOnly::new(bytes.into_bytes());
        assert_eq!(read_request(&mut reader).err(), Some("HTTP_HEADER_MISSING"));
        assert!(!reader.body_attempted);
    }
    #[test]
    fn http_security_regression_unknown_event_is_refused_before_body_read() {
        let bytes = String::from_utf8(raw("", "2", "{}")).unwrap().replace("X-GitHub-Event: issue_comment", "X-GitHub-Event: ping");
        let mut reader = HeadersOnly::new(bytes.into_bytes());
        assert_eq!(read_request(&mut reader).err(), Some("HTTP_EVENT_UNSUPPORTED"));
        assert!(!reader.body_attempted);
    }
    #[test]
    fn http_security_regression_bad_mac_shape_is_refused_before_body_read() {
        let mut reader = HeadersOnly::new(raw("", "2", "{}"));
        assert_eq!(read_request(&mut reader).err(), Some("HTTP_SIGNATURE_FORMAT"));
        assert!(!reader.body_attempted);
    }
}
