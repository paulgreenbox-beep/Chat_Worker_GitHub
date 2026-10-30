//! Pure HTTP-reader tests. No HMAC/API/TLS/Windows qualification is implied.
use cwg_github::{MAX_EVENT_BYTES, http::read_request};
use std::io::{self, Read};

fn header(length: usize, extra: &str) -> Vec<u8> {
    format!(
        "POST /github HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nX-GitHub-Event: issue_comment\r\nX-GitHub-Delivery: owned-review\r\nX-Hub-Signature-256: sha256={}\r\nContent-Length: {length}\r\n{extra}\r\n",
        "0".repeat(64)
    )
    .into_bytes()
}
struct Fragmented<'a> {
    remaining: &'a [u8],
    chunk: usize,
}
impl Read for Fragmented<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let count = self.chunk.min(output.len()).min(self.remaining.len());
        output[..count].copy_from_slice(&self.remaining[..count]);
        self.remaining = &self.remaining[count..];
        Ok(count)
    }
}

#[test]
fn arbitrary_short_reads_preserve_signature_input_bytes_exactly() {
    let body = "{\"body\":\"Привет\\nworld\"}".as_bytes();
    let mut wire = header(body.len(), "");
    wire.extend_from_slice(body);
    for chunk in [1, 2, 3, 7, 64] {
        let mut reader = Fragmented {
            remaining: &wire,
            chunk,
        };
        let request = read_request(&mut reader).unwrap();
        assert_eq!(request.body, body);
        assert!(reader.remaining.is_empty());
    }
}

#[test]
fn maximum_body_is_accepted_and_one_more_byte_is_refused_before_read() {
    let mut maximum = header(MAX_EVENT_BYTES, "");
    maximum.extend(std::iter::repeat_n(b' ', MAX_EVENT_BYTES));
    assert_eq!(
        read_request(&mut maximum.as_slice()).unwrap().body.len(),
        MAX_EVENT_BYTES
    );
    let oversized = header(MAX_EVENT_BYTES + 1, "");
    // No body is present: refusal must be size, not truncation while reading it.
    assert_eq!(
        read_request(&mut oversized.as_slice()).err(),
        Some("HTTP_BODY_SIZE")
    );
}

#[test]
fn mixed_case_duplicates_fail_even_when_values_are_identical() {
    for extra in [
        "hOsT: localhost\r\n",
        "X-github-Event: issue_comment\r\n",
        "X-GITHUB-DELIVERY: owned-review\r\n",
    ] {
        let wire = header(2, extra);
        assert_eq!(
            read_request(&mut wire.as_slice()).err(),
            Some("HTTP_DUPLICATE_HEADER")
        );
    }
}

#[test]
fn exact_content_length_does_not_consume_a_subsequent_frame() {
    let mut wire = header(2, "");
    wire.extend_from_slice(b"{}NEXT");
    let mut reader = Fragmented {
        remaining: &wire,
        chunk: 2,
    };
    assert_eq!(read_request(&mut reader).unwrap().body, b"{}");
    // The host closes after one request; this is not pipelining support.
    assert_eq!(reader.remaining, b"NEXT");
}
