//! Local HTTP server and temporary directory for modding tests.

use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

#[derive(Clone)]
pub(crate) struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    mode: Mode,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Sized,
    Chunked,
    // Declares a longer body than it sends, then keeps the connection open.
    Stall,
}

impl Reply {
    pub(crate) fn ok(body: Vec<u8>) -> Self {
        Self::new(200, body, Mode::Sized)
    }

    pub(crate) fn chunked(body: Vec<u8>) -> Self {
        Self::new(200, body, Mode::Chunked)
    }

    pub(crate) fn stall_after(body: Vec<u8>) -> Self {
        Self::new(200, body, Mode::Stall)
    }

    pub(crate) fn status(status: u16) -> Self {
        Self::new(status, Vec::new(), Mode::Sized)
    }

    pub(crate) fn redirect(location: &str) -> Self {
        let mut reply = Self::new(302, Vec::new(), Mode::Sized);
        reply
            .headers
            .push(("Location".to_owned(), location.to_owned()));
        reply
    }

    fn new(status: u16, body: Vec<u8>, mode: Mode) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body,
            mode,
        }
    }
}

/// Serves registered paths over plain HTTP on 127.0.0.1 until dropped.
pub(crate) struct TestServer {
    address: String,
    routes: Arc<Mutex<HashMap<String, Reply>>>,
    requests: Arc<Mutex<Vec<(String, String)>>>,
    stop: Arc<AtomicBool>,
}

impl TestServer {
    pub(crate) fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let routes = Arc::new(Mutex::new(HashMap::new()));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let accept = (routes.clone(), requests.clone(), stop.clone());
        thread::spawn(move || {
            let (routes, requests, stop) = accept;
            for stream in listener.incoming() {
                if stop.load(Ordering::Acquire) {
                    break;
                }
                let Ok(stream) = stream else { continue };
                let (routes, requests, stop) = (routes.clone(), requests.clone(), stop.clone());
                thread::spawn(move || serve(stream, &routes, &requests, &stop));
            }
        });
        Self {
            address,
            routes,
            requests,
            stop,
        }
    }

    pub(crate) fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.address)
    }

    pub(crate) fn set(&self, path: &str, reply: Reply) {
        self.routes.lock().unwrap().insert(path.to_owned(), reply);
    }

    /// Paths requested so far, in order.
    pub(crate) fn requests(&self) -> Vec<String> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .map(|(path, _)| path.clone())
            .collect()
    }

    /// Raw request heads (lowercased) received for `path`.
    pub(crate) fn headers(&self, path: &str) -> Vec<String> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|(requested, _)| requested == path)
            .map(|(_, head)| head.to_ascii_lowercase())
            .collect()
    }

    pub(crate) fn clear_requests(&self) {
        self.requests.lock().unwrap().clear();
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        // Wakes the accept loop so it can exit.
        let _ = TcpStream::connect(&self.address);
    }
}

fn serve(
    mut stream: TcpStream,
    routes: &Mutex<HashMap<String, Reply>>,
    requests: &Mutex<Vec<(String, String)>>,
    stop: &AtomicBool,
) {
    if stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .is_err()
    {
        return;
    }
    let mut head = Vec::new();
    let mut byte = [0];
    while !head.ends_with(b"\r\n\r\n") {
        if stream.read_exact(&mut byte).is_err() {
            return;
        }
        head.push(byte[0]);
    }
    let head = String::from_utf8_lossy(&head).into_owned();
    let path = head
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_owned();
    requests.lock().unwrap().push((path.clone(), head));
    let reply = routes
        .lock()
        .unwrap()
        .get(&path)
        .cloned()
        .unwrap_or_else(|| Reply::status(404));
    let mut response = format!("HTTP/1.1 {} Test\r\nConnection: close\r\n", reply.status);
    for (name, value) in &reply.headers {
        response.push_str(&format!("{name}: {value}\r\n"));
    }
    // Clients may hang up early on oversized bodies, so write errors are expected.
    match reply.mode {
        Mode::Sized => {
            response.push_str(&format!("Content-Length: {}\r\n\r\n", reply.body.len()));
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.write_all(&reply.body);
        }
        Mode::Chunked => {
            response.push_str("Transfer-Encoding: chunked\r\n\r\n");
            let _ = stream.write_all(response.as_bytes());
            for part in reply.body.chunks(512) {
                if stream
                    .write_all(format!("{:x}\r\n", part.len()).as_bytes())
                    .and_then(|()| stream.write_all(part))
                    .and_then(|()| stream.write_all(b"\r\n"))
                    .is_err()
                {
                    return;
                }
            }
            let _ = stream.write_all(b"0\r\n\r\n");
        }
        Mode::Stall => {
            response.push_str(&format!(
                "Content-Length: {}\r\n\r\n",
                reply.body.len() + 1000
            ));
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.write_all(&reply.body);
            for _ in 0..50 {
                if stop.load(Ordering::Acquire) {
                    return;
                }
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

pub(crate) struct TempDir(PathBuf);

impl TempDir {
    pub(crate) fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("legio-modding-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
