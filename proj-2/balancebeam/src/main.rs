mod request;
mod response;

use clap::Clap;
use rand::{Rng, SeedableRng};
use std::io::{Error, ErrorKind};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tokio::time;

/// Contains information parsed from the command-line invocation of balancebeam. The Clap macros
/// provide a fancy way to automatically construct a command-line argument parser.
#[derive(Clap, Debug)]
#[clap(about = "Fun with load balancing")]
struct CmdOptions {
    #[clap(
        short,
        long,
        about = "IP/port to bind to",
        default_value = "0.0.0.0:1100"
    )]
    bind: String,
    #[clap(short, long, about = "Upstream host to forward requests to")]
    upstream: Vec<String>,
    #[clap(
        long,
        about = "Perform active health checks on this interval (in seconds)",
        default_value = "10"
    )]
    active_health_check_interval: usize,
    #[clap(
        long,
        about = "Path to send request to for active health checks",
        default_value = "/"
    )]
    active_health_check_path: String,
    #[clap(
        long,
        about = "Maximum number of requests to accept per IP per minute (0 = unlimited)",
        default_value = "0"
    )]
    max_requests_per_minute: usize,
}

#[derive(Debug)]
struct UpstreamAddress {
    address: String,
    alive: bool,
}

/// Contains information about the state of balancebeam (e.g. what servers we are currently proxying
/// to, what servers have failed, rate limiting counts, etc.)
///
/// You should add fields to this struct in later milestones.
struct ProxyState {
    /// How frequently we check whether upstream servers are alive (Milestone 4)
    #[allow(dead_code)]
    active_health_check_interval: usize,
    /// Where we should send requests when doing active health checks (Milestone 4)
    #[allow(dead_code)]
    active_health_check_path: String,
    /// Maximum number of requests an individual IP can make in a minute (Milestone 5)
    #[allow(dead_code)]
    max_requests_per_minute: usize,
    /// Addresses of servers that we are proxying to
    upstream_addresses: RwLock<Vec<UpstreamAddress>>,
}

#[tokio::main]
async fn main() {
    use std::sync::Arc;
    // Initialize the logging library. You can print log messages using the `log` macros:
    // https://docs.rs/log/0.4.8/log/ You are welcome to continue using print! statements; this
    // just looks a little prettier.
    if let Err(_) = std::env::var("RUST_LOG") {
        std::env::set_var("RUST_LOG", "debug");
    }
    pretty_env_logger::init();

    // Parse the command line arguments passed to this program
    let options = CmdOptions::parse();
    if options.upstream.len() < 1 {
        log::error!("At least one upstream server must be specified using the --upstream option.");
        std::process::exit(1);
    }

    // Start listening for connections
    let listener = match TcpListener::bind(&options.bind).await {
        Ok(listener) => listener,
        Err(err) => {
            log::error!("Could not bind to {}: {}", options.bind, err);
            std::process::exit(1);
        }
    };
    log::info!("Listening for requests on {}", options.bind);

    // Handle incoming connections
    let state = ProxyState {
        upstream_addresses: RwLock::new(
            options
                .upstream
                .iter()
                .map(|address| UpstreamAddress {
                    address: address.to_string(),
                    alive: true,
                })
                .collect::<Vec<UpstreamAddress>>(),
        ),
        active_health_check_interval: options.active_health_check_interval,
        active_health_check_path: options.active_health_check_path,
        max_requests_per_minute: options.max_requests_per_minute,
    };
    let state_arc = Arc::new(state);

    let state_clone = state_arc.clone();
    tokio::spawn(async move {
        let mut interval = time::interval(time::Duration::from_secs(
            state_clone.active_health_check_interval as u64,
        ));
        loop {
            interval.tick().await;
            active_health_checks(&state_clone).await;
        }
    });

    loop {
        let (socket, _) = listener.accept().await.unwrap();
        let state = state_arc.clone();
        tokio::spawn(async move {
            handle_connection(socket, &state).await;
        });
    }
}

async fn active_health_checks(state: &ProxyState) {
    log::info!("Starting active health checks....");
    let mut dead_upstreams: Vec<String> = Vec::new();
    let mut live_upstreams: Vec<String> = Vec::new();
    {
        let addresses = state.upstream_addresses.read().await;
        for addr in addresses.iter() {
            let mut is_alive = addr.alive;
            let request = http::Request::builder()
                .method(http::Method::GET)
                .uri(&state.active_health_check_path)
                .header("Host", &addr.address)
                .body(Vec::new())
                .unwrap();
            match TcpStream::connect(&addr.address).await {
                Ok(mut stream) => {
                    if let Err(e) = request::write_to_stream(&request, &mut stream).await {
                        log::error!("Failed to write to upstream {}", e);
                        is_alive = false;
                    }
                    match response::read_from_stream(&mut stream, &http::Method::GET).await {
                        Ok(response) => {
                            is_alive = response.status().as_u16() == 200;
                        }
                        Err(e) => {
                            log::error!("Error reading from upstream {:?}", e);
                            is_alive = false;
                        }
                    }
                }
                Err(e) => {
                    log::error!(
                        "Failed to connect to upstream {} {}. Marking it dead",
                        &addr.address,
                        e
                    );
                    is_alive = false;
                }
            }
            if is_alive != addr.alive {
                if is_alive {
                    live_upstreams.push(addr.address.clone());
                } else {
                    dead_upstreams.push(addr.address.clone());
                }
            }
        }
    }
    for addr in dead_upstreams {
        mark_upstream_status(state, addr, false).await;
    }
    for addr in live_upstreams {
        mark_upstream_status(state, addr, true).await;
    }
    log::info!("Active health checks complete.");
}

async fn get_live_upstream(state: &ProxyState) -> Option<String> {
    let mut rng = rand::rngs::StdRng::from_entropy();
    let addresses = state.upstream_addresses.read().await;
    let live_addresses = addresses
        .iter()
        .filter(|addr| addr.alive)
        .collect::<Vec<&UpstreamAddress>>();
    return if live_addresses.is_empty() {
        None
    } else {
        let upstream_idx = rng.gen_range(0..live_addresses.len());
        Some(live_addresses[upstream_idx].address.clone())
    };
}

async fn mark_upstream_status(state: &ProxyState, address: String, is_alive: bool) {
    let mut addresses = state.upstream_addresses.write().await;
    for addr in addresses.iter_mut() {
        if addr.address == address {
            addr.alive = is_alive;
        }
    }
    log::info!("Upstreams {:?}", addresses);
}

async fn connect_to_upstream(state: &ProxyState) -> Result<TcpStream, std::io::Error> {
    loop {
        if let Some(upstream_ip) = get_live_upstream(state).await {
            match TcpStream::connect(&upstream_ip).await {
                Ok(stream) => break Ok(stream),
                Err(e) => {
                    log::error!("Failed to connect to upstream {}: {}", upstream_ip, e);
                    mark_upstream_status(state, upstream_ip, false).await;
                    continue;
                }
            }
        } else {
            log::error!("No live upstreams available");
            break Err(Error::new(ErrorKind::NotConnected, "No live upstreams"));
        }
    }
}

async fn send_response(client_conn: &mut TcpStream, response: &http::Response<Vec<u8>>) {
    let client_ip = client_conn.peer_addr().unwrap().ip().to_string();
    log::info!(
        "{} <- {}",
        client_ip,
        response::format_response_line(&response)
    );
    if let Err(error) = response::write_to_stream(&response, client_conn).await {
        log::warn!("Failed to send response to client: {}", error);
        return;
    }
}

async fn handle_connection(mut client_conn: TcpStream, state: &ProxyState) {
    let client_ip = client_conn.peer_addr().unwrap().ip().to_string();
    log::info!("Connection received from {}", client_ip);

    // Open a connection to a random destination server
    let mut upstream_conn = match connect_to_upstream(state).await {
        Ok(stream) => stream,
        Err(_error) => {
            let response = response::make_http_error(http::StatusCode::BAD_GATEWAY);
            send_response(&mut client_conn, &response).await;
            return;
        }
    };
    let upstream_ip = client_conn.peer_addr().unwrap().ip().to_string();

    // The client may now send us one or more requests. Keep trying to read requests until the
    // client hangs up or we get an error.
    loop {
        // Read a request from the client
        let mut request = match request::read_from_stream(&mut client_conn).await {
            Ok(request) => request,
            // Handle case where client closed connection and is no longer sending requests
            Err(request::Error::IncompleteRequest(0)) => {
                log::debug!("Client finished sending requests. Shutting down connection");
                return;
            }
            // Handle I/O error in reading from the client
            Err(request::Error::ConnectionError(io_err)) => {
                log::info!("Error reading request from client stream: {}", io_err);
                return;
            }
            Err(error) => {
                log::debug!("Error parsing request: {:?}", error);
                let response = response::make_http_error(match error {
                    request::Error::IncompleteRequest(_)
                    | request::Error::MalformedRequest(_)
                    | request::Error::InvalidContentLength
                    | request::Error::ContentLengthMismatch => http::StatusCode::BAD_REQUEST,
                    request::Error::RequestBodyTooLarge => http::StatusCode::PAYLOAD_TOO_LARGE,
                    request::Error::ConnectionError(_) => http::StatusCode::SERVICE_UNAVAILABLE,
                });
                send_response(&mut client_conn, &response).await;
                continue;
            }
        };
        log::info!(
            "{} -> {}: {}",
            client_ip,
            upstream_ip,
            request::format_request_line(&request)
        );

        // Add X-Forwarded-For header so that the upstream server knows the client's IP address.
        // (We're the ones connecting directly to the upstream server, so without this header, the
        // upstream server will only know our IP, not the client's.)
        request::extend_header_value(&mut request, "x-forwarded-for", &client_ip);

        // Forward the request to the server
        if let Err(error) = request::write_to_stream(&request, &mut upstream_conn).await {
            log::error!(
                "Failed to send request to upstream {}: {}",
                upstream_ip,
                error
            );
            let response = response::make_http_error(http::StatusCode::BAD_GATEWAY);
            send_response(&mut client_conn, &response).await;
            return;
        }
        log::debug!("Forwarded request to server");

        // Read the server's response
        let response = match response::read_from_stream(&mut upstream_conn, request.method()).await
        {
            Ok(response) => response,
            Err(error) => {
                log::error!("Error reading response from server: {:?}", error);
                let response = response::make_http_error(http::StatusCode::BAD_GATEWAY);
                send_response(&mut client_conn, &response).await;
                return;
            }
        };
        // Forward the response to the client
        send_response(&mut client_conn, &response).await;
        log::debug!("Forwarded response to client");
    }
}
