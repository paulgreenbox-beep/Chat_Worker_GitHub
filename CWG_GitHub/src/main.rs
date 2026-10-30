use cwg_github::Config;
use cwg_security::Result;
use cwg_state::Store;
use std::path::Path;
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
    let config = config_from_file(config_path)?;
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
fn config_from_file(path: &Path) -> Result<Config> {
    let bytes = cwg_security::windows_acl::read_private_config(path, 65_536)?;
    Config::from_bytes(&bytes)
}
#[cfg(not(windows))]
fn serve(_: Config) -> Result<()> { Err("WINDOWS_RECEIVER_REQUIRED") }

#[cfg(windows)]
fn serve(config: Config) -> Result<()> {
    use std::{net::TcpListener, time::{Duration, Instant}};
    if !config.enabled { return Err("RECEIVER_DISABLED"); }
    let secret = std::env::var(&config.webhook_secret_env).map_err(|_| "WEBHOOK_SECRET_MISSING")?;
    if !(16..=1024).contains(&secret.len()) { return Err("WEBHOOK_SECRET_SIZE"); }
    let token = std::env::var(&config.github_token_env).map_err(|_| "GITHUB_TOKEN_MISSING")?;
    windows::Client::new(&token, Instant::now() + Duration::from_secs(8))?;
    let store = open_store(&config)?;
    let listener = TcpListener::bind(config.listen).map_err(|_| "LISTEN_FAILED")?;
    eprintln!("CWG loopback receiver started explicitly; chat dispatch is NOT_IMPLEMENTED");
    for stream in listener.incoming() {
        let mut stream = stream.map_err(|_| "ACCEPT_FAILED")?;
        stream.set_write_timeout(Some(Duration::from_secs(2))).map_err(|_| "WRITE_TIMEOUT_SETUP")?;
        let outcome = receive_one(&mut stream, &config, &store, secret.as_bytes(), || {
            windows::Client::new(&token, Instant::now() + Duration::from_secs(7))
        }, Instant::now() + Duration::from_secs(5));
        if let Err(code) = outcome { eprintln!("CWG request refused: {code}"); }
        // A failed response write never rolls back/retries a committed enqueue.
        let _ = write_response(&mut stream, outcome);
    }
    Ok(())
}

#[cfg(windows)]
fn open_store(config: &Config) -> Result<Store> {
    let store = Store::open(&config.state_directory)?;
    for route in &config.routes { store.register(&route.binding)?; }
    Ok(store)
}

#[cfg(windows)]
fn receive_one<A: cwg_github::GithubApi>(
    stream: &mut std::net::TcpStream, config: &Config, store: &Store, secret: &[u8],
    api_factory: impl FnOnce() -> Result<A>, end: std::time::Instant,
) -> Result<bool> {
    let request = cwg_github::http::read_stream_request(stream, end)?;
    let api = api_factory()?;
    cwg_github::Ingress { config, api: &api, store, secret }
        .receive(&request.event, &request.delivery, &request.signature, &request.body)
}

#[cfg(windows)]
fn write_response(stream: &mut std::net::TcpStream, outcome: Result<bool>) -> std::io::Result<()> {
    use std::io::Write;
    let (status, body) = match outcome {
        Ok(true) => ("202 Accepted", "{\"status\":\"VERIFIED\",\"chat_delivered\":false}"),
        Ok(false) => ("200 OK", "{\"status\":\"DUPLICATE\",\"chat_delivered\":false}"),
        Err(code) if ["GITHUB_", "STATE_", "JOURNAL_", "CNG_", "ACL_", "PRIVATE_", "WINDOWS_"].iter().any(|prefix| code.starts_with(prefix)) =>
            ("503 Service Unavailable", "{\"error\":\"temporarily_unavailable\"}"),
        Err(_) => ("403 Forbidden", "{\"error\":\"rejected\"}"),
    };
    write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
}

#[cfg(all(test, windows))]
mod native_tests;
