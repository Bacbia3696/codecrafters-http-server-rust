use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::Arc;
use std::thread;

enum ResponseBody {
    Text(String),
    Binary(Vec<u8>),
}

struct Response {
    status: String,
    content_type: String,
    body: ResponseBody,
}

impl Response {
    fn to_bytes(&self) -> Vec<u8> {
        let body_bytes = match &self.body {
            ResponseBody::Text(s) => s.as_bytes().to_vec(),
            ResponseBody::Binary(b) => b.clone(),
        };
        let mut response = format!(
            "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\n\r\n",
            self.status,
            self.content_type,
            body_bytes.len()
        )
        .into_bytes();
        response.extend(body_bytes);
        response
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut directory = String::new();

    let mut i = 1;
    while i < args.len() {
        if args[i] == "--directory" && i + 1 < args.len() {
            directory = args[i + 1].clone();
            i += 2;
        } else {
            i += 1;
        }
    }

    let directory = Arc::new(directory);
    let listener = TcpListener::bind("127.0.0.1:4221").unwrap();
    println!("Server is listening on 127.0.0.1:4221...");

    loop {
        let stream = listener.accept();
        let directory = Arc::clone(&directory);
        thread::spawn(move || {
            // Handle the stream
            match stream {
                Ok((mut stream, client_addr)) => {
                    println!("New connection from {}", client_addr);

                    let mut buffer = [0u8; 1024];
                    let bytes_read = stream.read(&mut buffer).unwrap();
                    let request = String::from_utf8_lossy(&buffer[..bytes_read]);

                    let response = handle_request(&request, &directory);
                    stream.write_all(&response.to_bytes()).unwrap();
                }
                Err(e) => {
                    println!("error: {}", e);
                }
            }
        });
    }
}

fn handle_request(request: &str, directory: &str) -> Response {
    let request_line = request.lines().next().unwrap_or("");
    let parts: Vec<&str> = request_line.split_whitespace().collect();

    if parts.len() >= 2 {
        let path = parts[1];
        if path == "/" {
            return Response {
                status: "200 OK".to_string(),
                content_type: "text/plain".to_string(),
                body: ResponseBody::Text(String::new()),
            };
        }
    }

    let path = parts.get(1).copied().unwrap_or("");
    if let Some(message) = path.strip_prefix("/echo/") {
        return Response {
            status: "200 OK".to_string(),
            content_type: "text/plain".to_string(),
            body: ResponseBody::Text(message.to_string()),
        };
    }

    if path == "/user-agent" {
        let user_agent = parse_header(request, "User-Agent").unwrap_or("");
        return Response {
            status: "200 OK".to_string(),
            content_type: "text/plain".to_string(),
            body: ResponseBody::Text(user_agent.to_string()),
        };
    }

    if let Some(filename) = path.strip_prefix("/files/") {
        let file_path = Path::new(directory).join(filename);
        if file_path.exists() && file_path.is_file() {
            match fs::read(&file_path) {
                Ok(contents) => {
                    return Response {
                        status: "200 OK".to_string(),
                        content_type: "application/octet-stream".to_string(),
                        body: ResponseBody::Binary(contents),
                    };
                }
                Err(_) => {
                    return Response {
                        status: "500 Internal Server Error".to_string(),
                        content_type: "text/plain".to_string(),
                        body: ResponseBody::Text(String::new()),
                    };
                }
            }
        } else {
            return Response {
                status: "404 Not Found".to_string(),
                content_type: "text/plain".to_string(),
                body: ResponseBody::Text(String::new()),
            };
        }
    }

    Response {
        status: "404 Not Found".to_string(),
        content_type: "text/plain".to_string(),
        body: ResponseBody::Text(String::new()),
    }
}

fn parse_header<'a>(request: &'a str, header_name: &str) -> Option<&'a str> {
    for line in request.lines() {
        if let Some((name, value)) = line.split_once(':')
            && name.trim().eq_ignore_ascii_case(header_name)
        {
            return Some(value.trim());
        }
    }
    None
}
