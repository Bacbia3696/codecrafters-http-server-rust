use std::io::{Read, Write};
use std::net::TcpListener;

fn main() {
    let listener = TcpListener::bind("127.0.0.1:4221").unwrap();
    println!("Server is listening on 127.0.0.1:4221...");

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                let client_addr = stream.peer_addr().unwrap();
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

    let path = parts[1];
    if let Some(message) = path.strip_prefix("/echo/") {
        return format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
            message.len(),
            message
        );
    }
    "HTTP/1.1 404 Not Found\r\n\r\n".to_string()
}
