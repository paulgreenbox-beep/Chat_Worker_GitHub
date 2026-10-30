//! Stdio MCP host: one JSON-RPC object per bounded UTF-8 line, stdout only JSON.
use cwg_mcp::{MAX_RPC_BYTES, Server};
use std::{
    io::{self, BufRead, BufReader, Write},
    path::Path,
};

fn main() {
    if let Err(code) = run() {
        eprintln!("CWG MCP refused: {code}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), &'static str> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("USAGE: cwg-mcp ABSOLUTE_RECEIVER_CONFIG BINDING_ID");
    }
    let binding = args[1].to_str().ok_or("BINDING_ID_INVALID")?;
    let mut server = Server::open(Path::new(&args[0]), binding)?;

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::with_capacity(8192, stdin.lock());
    let mut writer = stdout.lock();
    loop {
        let Some(line) = read_line(&mut reader).map_err(|_| "MCP_INPUT_FRAMING")? else {
            break;
        };
        if let Some(reply) = server.handle_json(&line) {
            serde_json::to_writer(&mut writer, &reply).map_err(|_| "MCP_OUTPUT_ERROR")?;
            writer.write_all(b"\n").map_err(|_| "MCP_OUTPUT_ERROR")?;
            writer.flush().map_err(|_| "MCP_OUTPUT_ERROR")?;
        }
    }
    Ok(())
}

fn read_line(reader: &mut impl BufRead) -> io::Result<Option<Vec<u8>>> {
    let mut data = Vec::new();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            if data.is_empty() {
                return Ok(None);
            }
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "partial JSON-RPC frame",
            ));
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let count = newline.map_or(available.len(), |at| at + 1);
        if data.len() + count > MAX_RPC_BYTES + 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "JSON-RPC frame too large",
            ));
        }
        data.extend_from_slice(&available[..count]);
        reader.consume(count);
        if newline.is_some() {
            data.pop();
            if data.last() == Some(&b'\r') {
                data.pop();
            }
            return Ok(Some(data));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn line_reader_accepts_newline_and_rejects_partial_eof() {
        let mut good = BufReader::new(&b"{\"id\":1}\r\n"[..]);
        assert_eq!(read_line(&mut good).unwrap(), Some(b"{\"id\":1}".to_vec()));
        assert_eq!(read_line(&mut good).unwrap(), None);
        let mut bad = BufReader::new(&b"{\"id\":1}"[..]);
        assert!(read_line(&mut bad).is_err());
    }
    #[test]
    fn oversized_frame_fails_before_unbounded_allocation() {
        let huge = vec![b'x'; MAX_RPC_BYTES + 2];
        let mut reader = BufReader::new(huge.as_slice());
        assert!(read_line(&mut reader).is_err());
    }
}
