use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::Arc;
use std::thread;

const BIND_ADDR: &str = "127.0.0.1:4221";

enum ResponseBody {
    Text(String),
    Binary(Vec<u8>),
}

struct Response {
    status: &'static str,
    content_type: Option<&'static str>,
    body: ResponseBody,
}

impl Response {
    fn to_bytes(&self) -> Vec<u8> {
        let body_bytes = match &self.body {
            ResponseBody::Text(s) => s.as_bytes().to_vec(),
            ResponseBody::Binary(b) => b.clone(),
        };

        let headers = match self.content_type {
            Some(ct) => format!(
                "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\n\r\n",
                self.status,
                ct,
                body_bytes.len()
            ),
            None => format!(
                "HTTP/1.1 {}\r\nContent-Length: {}\r\n\r\n",
                self.status,
                body_bytes.len()
            ),
        };

        [headers.into_bytes(), body_bytes].concat()
    }
}

fn ok_text(body: String) -> Response {
    Response {
        status: "200 OK",
        content_type: Some("text/plain"),
        body: ResponseBody::Text(body),
    }
}

fn ok_binary(body: Vec<u8>) -> Response {
    Response {
        status: "200 OK",
        content_type: Some("application/octet-stream"),
        body: ResponseBody::Binary(body),
    }
}

fn created() -> Response {
    Response {
        status: "201 Created",
        content_type: None,
        body: ResponseBody::Text(String::new()),
    }
}

fn not_found() -> Response {
    Response {
        status: "404 Not Found",
        content_type: None,
        body: ResponseBody::Text(String::new()),
    }
}

fn internal_error() -> Response {
    Response {
        status: "500 Internal Server Error",
        content_type: None,
        body: ResponseBody::Text(String::new()),
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let directory = parse_directory_arg(&args).unwrap_or_default();
    let directory = Arc::new(directory);

    let listener = TcpListener::bind(BIND_ADDR).unwrap();
    println!("Server is listening on {}...", BIND_ADDR);

    for stream in listener.incoming() {
        let directory = Arc::clone(&directory);
        thread::spawn(move || match stream {
            Ok(mut stream) => {
                let client_addr = stream.peer_addr().unwrap();
                println!("New connection from {}", client_addr);
                handle_connection(&mut stream, &directory);
            }
            Err(e) => println!("error: {}", e),
        });
    }
}

fn parse_directory_arg(args: &[String]) -> Option<String> {
    args.windows(2).find_map(|w| {
        if w[0] == "--directory" {
            Some(w[1].clone())
        } else {
            None
        }
    })
}

fn handle_connection(stream: &mut (impl Read + Write), directory: &str) {
    let mut buffer = [0u8; 4096];
    let bytes_read = stream.read(&mut buffer).unwrap();
    let request = String::from_utf8_lossy(&buffer[..bytes_read]);

    let (method, path) = parse_request_line(&request);
    let body = extract_body(&request, &buffer, bytes_read);

    let response = handle_request(&method, &path, &request, directory, body);
    stream.write_all(&response.to_bytes()).unwrap();
}

fn parse_request_line(request: &str) -> (String, String) {
    let parts: Vec<&str> = request
        .lines()
        .next()
        .unwrap_or("")
        .split_whitespace()
        .collect();
    let method = parts.first().copied().unwrap_or("").to_string();
    let path = parts.get(1).copied().unwrap_or("").to_string();
    (method, path)
}

fn extract_body<'a>(request: &'a str, buffer: &'a [u8], bytes_read: usize) -> &'a [u8] {
    request
        .find("\r\n\r\n")
        .map(|i| &buffer[(i + 4)..bytes_read])
        .unwrap_or(&[])
}

fn handle_request(
    method: &str,
    path: &str,
    request: &str,
    directory: &str,
    body: &[u8],
) -> Response {
    match path {
        "/" => ok_text(String::new()),
        p if p.starts_with("/echo/") => ok_text(p.strip_prefix("/echo/").unwrap().to_string()),
        "/user-agent" => {
            let ua = parse_header(request, "User-Agent").unwrap_or("");
            ok_text(ua.to_string())
        }
        p if p.starts_with("/files/") => handle_files(method, p, directory, body),
        _ => not_found(),
    }
}

fn handle_files(method: &str, path: &str, directory: &str, body: &[u8]) -> Response {
    let filename = path.strip_prefix("/files/").unwrap();
    let file_path = Path::new(directory).join(filename);

    match method.to_lowercase().as_str() {
        "get" => {
            if !file_path.exists() || !file_path.is_file() {
                return not_found();
            }
            fs::read(&file_path)
                .map(ok_binary)
                .unwrap_or_else(|_| internal_error())
        }
        "post" => fs::write(&file_path, body)
            .map(|_| created())
            .unwrap_or_else(|_| internal_error()),
        _ => not_found(),
    }
}

fn parse_header<'a>(request: &'a str, header_name: &str) -> Option<&'a str> {
    request.lines().find_map(|line| {
        line.split_once(':').and_then(|(name, value)| {
            if name.trim().eq_ignore_ascii_case(header_name) {
                Some(value.trim())
            } else {
                None
            }
        })
    })
}
