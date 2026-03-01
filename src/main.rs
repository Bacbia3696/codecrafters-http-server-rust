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
    content_type: Option<String>,
    body: ResponseBody,
}

impl Response {
    fn to_bytes(&self) -> Vec<u8> {
        let body_bytes = match &self.body {
            ResponseBody::Text(s) => s.as_bytes().to_vec(),
            ResponseBody::Binary(b) => b.clone(),
        };
        let mut response = match &self.content_type {
            Some(content_type) => format!(
                "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\n\r\n",
                self.status,
                content_type,
                body_bytes.len()
            )
            .into_bytes(),
            None => format!(
                "HTTP/1.1 {}\r\nContent-Length: {}\r\n\r\n",
                self.status,
                body_bytes.len()
            )
            .into_bytes(),
        };
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

                    let mut buffer = [0u8; 4096];
                    let bytes_read = stream.read(&mut buffer).unwrap();
                    let request = String::from_utf8_lossy(&buffer[..bytes_read]);

                    let (method, path) = parse_request_line(&request);
                    let content_length = parse_content_length(&request);
                    let body_start = request.find("\r\n\r\n").map(|i| i + 4).unwrap_or(0);
                    let body = if body_start < bytes_read {
                        &buffer[body_start..bytes_read]
                    } else {
                        &[]
                    };

                    let response = handle_request(&method, &path, &request, &directory, content_length, body);
                    stream.write_all(&response.to_bytes()).unwrap();
                }
                Err(e) => {
                    println!("error: {}", e);
                }
            }
        });
    }
}

fn parse_request_line(request: &str) -> (String, String) {
    let request_line = request.lines().next().unwrap_or("");
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    let method = parts.first().copied().unwrap_or("").to_string();
    let path = parts.get(1).copied().unwrap_or("").to_string();
    (method, path)
}

fn parse_content_length(request: &str) -> Option<usize> {
    parse_header(request, "Content-Length").and_then(|v| v.parse().ok())
}

fn handle_request(
    method: &str,
    path: &str,
    request: &str,
    directory: &str,
    _content_length: Option<usize>,
    body: &[u8],
) -> Response {
    if path == "/" {
        return Response {
            status: "200 OK".to_string(),
            content_type: Some("text/plain".to_string()),
            body: ResponseBody::Text(String::new()),
        };
    }

    if let Some(message) = path.strip_prefix("/echo/") {
        return Response {
            status: "200 OK".to_string(),
            content_type: Some("text/plain".to_string()),
            body: ResponseBody::Text(message.to_string()),
        };
    }

    if path == "/user-agent" {
        let user_agent = parse_header(request, "User-Agent").unwrap_or("");
        return Response {
            status: "200 OK".to_string(),
            content_type: Some("text/plain".to_string()),
            body: ResponseBody::Text(user_agent.to_string()),
        };
    }

    if let Some(filename) = path.strip_prefix("/files/") {
        let file_path = Path::new(directory).join(filename);

        match method.to_lowercase().as_str() {
            "get" => {
                if !file_path.exists() || !file_path.is_file() {
                    return Response {
                        status: "404 Not Found".to_string(),
                        content_type: None,
                        body: ResponseBody::Text(String::new()),
                    };
                }
                match fs::read(&file_path) {
                    Ok(contents) => Response {
                        status: "200 OK".to_string(),
                        content_type: Some("application/octet-stream".to_string()),
                        body: ResponseBody::Binary(contents),
                    },
                    Err(_) => Response {
                        status: "500 Internal Server Error".to_string(),
                        content_type: None,
                        body: ResponseBody::Text(String::new()),
                    },
                }
            }
            "post" => {
                match fs::write(&file_path, body) {
                    Ok(_) => Response {
                        status: "201 Created".to_string(),
                        content_type: None,
                        body: ResponseBody::Text(String::new()),
                    },
                    Err(_) => Response {
                        status: "500 Internal Server Error".to_string(),
                        content_type: None,
                        body: ResponseBody::Text(String::new()),
                    },
                }
            }
            _ => Response {
                status: "405 Method Not Allowed".to_string(),
                content_type: None,
                body: ResponseBody::Text(String::new()),
            },
        }
    } else {
        Response {
            status: "404 Not Found".to_string(),
            content_type: None,
            body: ResponseBody::Text(String::new()),
        }
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
