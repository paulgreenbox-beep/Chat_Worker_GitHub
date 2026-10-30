use cwg_github::Config;
use cwg_security::Result;
use cwg_state::Store;
use std::{fs::File, io::Read, path::Path};
#[cfg(windows)]
mod windows;

fn main() {
    if let Err(code) = run() {
        eprintln!("CWG refused: {code}");
        std::process::exit(2);
    }
}
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if !(args.len() == 2 || args.len() == 3) { return Err("USAGE: cwg-github check-config|serve CONFIG; cwg-github inbox CONFIG BINDING"); }
    let config_path = Path::new(&args[1]);
    if !config_path.is_absolute() { return Err("ABSOLUTE_CONFIG_REQUIRED"); }
    let metadata = std::fs::symlink_metadata(config_path).map_err(|_| "CONFIG_OPEN")?;
    if !metadata.is_file() || metadata.file_type().is_symlink() { return Err("CONFIG_REGULAR_FILE_REQUIRED"); }
    #[cfg(windows)]
    { use std::os::windows::fs::MetadataExt; if metadata.file_attributes() & 0x400 != 0 { return Err("CONFIG_REPARSE_REFUSED"); } }
    let mut bytes = Vec::new();
    File::open(config_path).map_err(|_| "CONFIG_OPEN")?.take(65_537).read_to_end(&mut bytes).map_err(|_| "CONFIG_READ")?;
    let config = Config::from_bytes(&bytes)?;
    match args[0].to_str() {
        Some("check-config") if args.len() == 2 => { println!("CONFIG_VALID; no service started; runtime NOT_VERIFIED"); Ok(()) }
        Some("inbox") if args.len() == 3 => {
            let id = args[2].to_str().ok_or("BINDING_INVALID")?;
            let route = config.routes.iter().find(|r| r.binding.id == id).ok_or("BINDING_UNKNOWN")?;
            let items = Store::open(&config.state_directory)?.inbox(&route.binding, 100)?;
            println!("{}", serde_json::to_string(&items).map_err(|_| "INBOX_ENCODE")?);
            Ok(())
        }
        Some("serve") if args.len() == 2 => serve(config),
        _ => Err("COMMAND_UNKNOWN"),
    }
}
#[cfg(not(windows))]
fn serve(_: Config) -> Result<()> { Err("WINDOWS_RECEIVER_REQUIRED") }

#[cfg(windows)]
fn serve(config: Config) -> Result<()> {
    use cwg_github::{Ingress, http::read_request};
    use std::{io::{self, Write}, net::{TcpListener, TcpStream}, time::{Duration, Instant}};
    if !config.enabled { return Err("RECEIVER_DISABLED"); }
    let secret = std::env::var(&config.webhook_secret_env).map_err(|_| "WEBHOOK_SECRET_MISSING")?;
    if !(16..=1024).contains(&secret.len()) { return Err("WEBHOOK_SECRET_SIZE"); }
    let token = std::env::var(&config.github_token_env).map_err(|_| "GITHUB_TOKEN_MISSING")?;
    windows::Client::new(&token, Instant::now() + Duration::from_secs(8))?;
    let store = Store::open(&config.state_directory)?;
    for route in &config.routes { store.register(&route.binding)?; }
    let listener = TcpListener::bind(config.listen).map_err(|_| "LISTEN_FAILED")?;
    eprintln!("CWG loopback receiver started explicitly; chat dispatch is NOT_IMPLEMENTED");
    struct Deadline<'a> { stream: &'a mut TcpStream, end: Instant }
    impl Read for Deadline<'_> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let remaining = self.end.saturating_duration_since(Instant::now());
            if remaining.is_zero() { return Err(io::Error::new(io::ErrorKind::TimedOut, "request deadline")); }
            self.stream.set_read_timeout(Some(remaining))?;
            self.stream.read(buf)
        }
    }
    for stream in listener.incoming() {
        let mut stream = stream.map_err(|_| "ACCEPT_FAILED")?;
        stream.set_write_timeout(Some(Duration::from_secs(2))).map_err(|_| "WRITE_TIMEOUT_SETUP")?;
        let outcome = (|| {
            let request = read_request(&mut Deadline { stream: &mut stream, end: Instant::now() + Duration::from_secs(5) })?;
            let api = windows::Client::new(&token, Instant::now() + Duration::from_secs(7))?;
            Ingress { config: &config, api: &api, store: &store, secret: secret.as_bytes() }
                .receive(&request.event, &request.delivery, &request.signature, &request.body)
        })();
        let (status, body) = match outcome {
            Ok(true) => ("202 Accepted", "{\"status\":\"VERIFIED\",\"chat_delivered\":false}"),
            Ok(false) => ("200 OK", "{\"status\":\"DUPLICATE\",\"chat_delivered\":false}"),
            Err(code) => {
                eprintln!("CWG request refused: {code}");
                if code.starts_with("GITHUB_") || code.starts_with("STATE_") || code.starts_with("JOURNAL_") {
                    ("503 Service Unavailable", "{\"error\":\"temporarily_unavailable\"}")
                } else { ("403 Forbidden", "{\"error\":\"rejected\"}") }
            }
        };
        // A failed response write never rolls back/retries a committed enqueue.
        let _ = write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
    }
    Ok(())
}
