use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

fn main() {
    let listener = TcpListener::bind("127.0.0.1:4221").unwrap();
    println!("Server is listening on 127.0.0.1:4221...");

    loop {
        let stream = listener.accept();
        thread::spawn(move || {
            // Handle the stream
            match stream {
                Ok((mut stream, client_addr)) => {
                    println!("New connection from {}", client_addr);

                    let mut buffer = [0u8; 1024];
                    let bytes_read = stream.read(&mut buffer).unwrap();
                    let request = String::from_utf8_lossy(&buffer[..bytes_read]);

                    let response = handle_request(&request);
                    stream.write_all(response.as_bytes()).unwrap();
                }
                Err(e) => {
                    println!("error: {}", e);
                }
            }
        });
    }
}

fn handle_request(request: &str) -> String {
    let request_line = request.lines().next().unwrap_or("");
    let parts: Vec<&str> = request_line.split_whitespace().collect();

    if parts.len() >= 2 {
        let path = parts[1];
        if path == "/" {
            return "HTTP/1.1 200 OK\r\n\r\n".to_string();
        }
    }

    let path = parts.get(1).copied().unwrap_or("");
    if let Some(message) = path.strip_prefix("/echo/") {
        return format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
            message.len(),
            message
        );
    }

    if path == "/user-agent" {
        let user_agent = parse_header(request, "User-Agent").unwrap_or("");
        return format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
            user_agent.len(),
            user_agent
        );
    }

    "HTTP/1.1 404 Not Found\r\n\r\n".to_string()
}

fn parse_header<'a>(request: &'a str, header_name: &str) -> Option<&'a str> {
    for line in request.lines() {
        if let Some((name, value)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case(header_name) {
                return Some(value.trim());
            }
        }
    }
    None
}
