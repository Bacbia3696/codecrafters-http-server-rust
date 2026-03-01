use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::Arc;
use std::thread;

use flate2::write::GzEncoder;
use flate2::Compression;

const BIND_ADDR: &str = "127.0.0.1:4221";

struct Request {
    method: String,
    path: String,
    #[allow(dead_code, unused)]
    query_params: HashMap<String, String>,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

impl Request {
    fn parse(raw: &[u8]) -> Result<Self, String> {
        let request_str = String::from_utf8_lossy(raw);
        let mut lines = request_str.lines();

        // Parse request line
        let request_line = lines.next().unwrap_or("");
        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or("").to_string();
        let full_path = parts.next().unwrap_or("");

        // Split path and query string
        let (path, query_params) = parse_path_and_query(full_path);

        // Parse headers
        let mut headers = HashMap::new();
        let mut body_start = 0;
        for line in lines {
            if line.is_empty() {
                body_start = request_str.find("\r\n\r\n").map(|i| i + 4).unwrap_or(0);
                break;
            }
            if let Some((key, value)) = line.split_once(':') {
                headers.insert(key.trim().to_string(), value.trim().to_string());
            }
        }

        // Extract body
        let body = if body_start > 0 {
            raw[body_start..].to_vec()
        } else {
            vec![]
        };

        Ok(Request {
            method,
            path,
            query_params,
            headers,
            body,
        })
    }

    fn get_header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

fn parse_path_and_query(full_path: &str) -> (String, HashMap<String, String>) {
    let mut query_params = HashMap::new();
    let path = full_path
        .split_once('?')
        .map(|(path, query)| {
            for param in query.split('&') {
                if let Some((key, value)) = param.split_once('=') {
                    query_params.insert(key.to_string(), value.to_string());
                }
            }
            path
        })
        .unwrap_or(full_path)
        .to_string();

    (path, query_params)
}

struct Response {
    status_code: u16,
    status_text: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

impl Response {
    fn new(status_code: u16, status_text: &str) -> Self {
        Response {
            status_code,
            status_text: status_text.to_string(),
            headers: HashMap::new(),
            body: vec![],
        }
    }

    fn with_content_type(mut self, content_type: &str) -> Self {
        self.headers
            .insert("Content-Type".to_string(), content_type.to_string());
        self
    }

    fn with_body(mut self, body: Vec<u8>) -> Self {
        self.body = body;
        self
    }

    fn with_text_body(mut self, text: String) -> Self {
        self.body = text.into_bytes();
        self
    }

    fn with_header(mut self, key: &str, value: &str) -> Self {
        self.headers.insert(key.to_string(), value.to_string());
        self
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut headers = format!("HTTP/1.1 {} {}\r\n", self.status_code, self.status_text);

        // Add Content-Length
        headers.push_str(&format!("Content-Length: {}\r\n", self.body.len()));

        // Add other headers
        for (key, value) in &self.headers {
            headers.push_str(&format!("{}: {}\r\n", key, value));
        }

        headers.push_str("\r\n");

        [headers.into_bytes(), self.body.clone()].concat()
    }
}

fn ok_text(body: String) -> Response {
    Response::new(200, "OK")
        .with_content_type("text/plain")
        .with_text_body(body)
}

fn ok_binary(body: Vec<u8>) -> Response {
    Response::new(200, "OK")
        .with_content_type("application/octet-stream")
        .with_body(body)
}

fn created() -> Response {
    Response::new(201, "Created")
}

fn not_found() -> Response {
    Response::new(404, "Not Found")
}

fn internal_error() -> Response {
    Response::new(500, "Internal Server Error")
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

    let request = match Request::parse(&buffer[..bytes_read]) {
        Ok(req) => req,
        Err(_) => {
            stream.write_all(&internal_error().to_bytes()).unwrap();
            return;
        }
    };

    let response = handle_request(&request, directory);
    stream.write_all(&response.to_bytes()).unwrap();
}

fn handle_request(request: &Request, directory: &str) -> Response {
    match request.path.as_str() {
        "/" => ok_text(String::new()),
        p if p.starts_with("/echo/") => {
            let echo_text = p.strip_prefix("/echo/").unwrap().to_string();
            let accepts_gzip = request
                .get_header("Accept-Encoding")
                .map(|e| e.contains("gzip"))
                .unwrap_or(false);

            if accepts_gzip {
                let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
                encoder.write_all(echo_text.as_bytes()).unwrap();
                let compressed = encoder.finish().unwrap();
                Response::new(200, "OK")
                    .with_content_type("text/plain")
                    .with_header("Content-Encoding", "gzip")
                    .with_body(compressed)
            } else {
                ok_text(echo_text)
            }
        }
        "/user-agent" => {
            let ua = request.get_header("User-Agent").unwrap_or("");
            ok_text(ua.to_string())
        }
        p if p.starts_with("/files/") => handle_files(&request.method, p, directory, &request.body),
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
