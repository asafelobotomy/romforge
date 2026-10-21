//! A stand-in web server on 127.0.0.1 for tests: answers each path with
//! what `answer` says (404 for `None`), and logs the paths asked for.
//! Several connections are served at once, as a real host would.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

pub fn serve(
    answer: impl Fn(&str) -> Option<Vec<u8>> + Send + Sync + 'static,
) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let log = Arc::new(Mutex::new(Vec::new()));
    let seen = log.clone();
    let answer = Arc::new(answer);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let (seen, answer) = (seen.clone(), answer.clone());
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() {
                    return;
                }
                loop {
                    let mut h = String::new();
                    if reader.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" {
                        break;
                    }
                }
                let path = line.split_whitespace().nth(1).unwrap_or("/").to_owned();
                seen.lock().unwrap().push(path.clone());
                let (status, body) = match answer(&path) {
                    Some(b) => ("200 OK", b),
                    None => ("404 Not Found", b"not found".to_vec()),
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(&body);
            });
        }
    });
    (base, log)
}
