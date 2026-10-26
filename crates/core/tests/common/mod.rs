//! A tiny fake website for tests. It runs on this computer, answers the addresses it was told
//! about, and remembers what it was asked, so tests can check both what the app did with a reply
//! and what the app sent.

#![allow(dead_code)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

pub struct Route {
    /// Either a path ("/stores") or a path with its query ("/deals?upperPrice=60"). The exact
    /// address is tried first, then the path alone.
    pub target: &'static str,
    pub status: u16,
    pub body: String,
    pub headers: Vec<(&'static str, String)>,
}

pub fn ok(target: &'static str, body: &str) -> Route {
    Route { target, status: 200, body: body.to_string(), headers: Vec::new() }
}

pub fn status(target: &'static str, status: u16, body: &str) -> Route {
    Route { target, status, body: body.to_string(), headers: Vec::new() }
}

#[derive(Debug, Clone)]
pub struct Seen {
    pub target: String,
    pub headers: Vec<(String, String)>,
}

impl Seen {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

pub struct Server {
    /// For example `http://127.0.0.1:50321`.
    pub base: String,
    pub seen: Arc<Mutex<Vec<Seen>>>,
}

impl Server {
    pub fn requests(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }
}

pub fn start(routes: Vec<Route>) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let routes = Arc::new(routes);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen_by_thread = seen.clone();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let routes = routes.clone();
            let seen = seen_by_thread.clone();
            thread::spawn(move || handle(stream, &routes, &seen));
        }
    });
    Server { base, seen }
}

fn handle(mut stream: TcpStream, routes: &[Route], seen: &Mutex<Vec<Seen>>) {
    let mut request = Vec::new();
    let mut buffer = [0u8; 1024];
    while !request.windows(4).any(|w| w == b"\r\n\r\n") {
        match stream.read(&mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(n) => request.extend_from_slice(&buffer[..n]),
        }
    }
    let text = String::from_utf8_lossy(&request).to_string();
    let mut lines = text.lines();
    let target = lines.next().unwrap_or("").split_whitespace().nth(1).unwrap_or("").to_string();
    let headers = lines
        .take_while(|l| !l.is_empty())
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    seen.lock().unwrap().push(Seen { target: target.clone(), headers });

    let path = target.split('?').next().unwrap_or("");
    let route = routes.iter().find(|r| r.target == target).or_else(|| routes.iter().find(|r| r.target == path));
    let (code, body, extra) = match route {
        Some(r) => (r.status, r.body.as_str(), r.headers.as_slice()),
        None => (404, "not found", &[][..]),
    };
    let mut response = format!(
        "HTTP/1.1 {code} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (name, value) in extra {
        response.push_str(&format!("{name}: {value}\r\n"));
    }
    response.push_str("\r\n");
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.write_all(body.as_bytes());
    let _ = stream.flush();
}