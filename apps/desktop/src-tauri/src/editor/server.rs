//! The preview WebSocket: listens on 127.0.0.1 at a random port, accepts only
//! clients that present the session token, and streams the newest frame to the
//! newest client. Frames that are replaced before they are sent are dropped.

use std::{
    io,
    net::{TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use tungstenite::{
    Bytes, Message, WebSocket,
    handshake::server::{ErrorResponse, Request, Response},
    http::StatusCode,
};

use super::protocol::Latest;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(2);
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(50);

pub struct PreviewServer {
    port: u16,
    token: String,
    frames: Arc<Latest<Bytes>>,
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

fn random_token() -> io::Result<String> {
    let mut bytes = [0u8; 24];
    getrandom::fill(&mut bytes).map_err(|e| io::Error::other(e.to_string()))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn query_token(request: &Request) -> Option<&str> {
    request
        .uri()
        .query()?
        .split('&')
        .find_map(|pair| pair.strip_prefix("token="))
}

fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0, |acc, (x, y)| acc | (x ^ y))
            == 0
}

fn reject() -> ErrorResponse {
    let mut response = ErrorResponse::new(Some("invalid token".into()));
    *response.status_mut() = StatusCode::FORBIDDEN;
    response
}

impl PreviewServer {
    pub fn start() -> io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        let token = random_token()?;
        let frames = Arc::new(Latest::default());
        let stop = Arc::new(AtomicBool::new(false));
        let (clients_tx, clients_rx) = mpsc::channel();

        let accept = {
            let token = token.clone();
            let stop = stop.clone();
            thread::Builder::new()
                .name("preview-accept".into())
                .spawn(move || accept_loop(listener, &token, &stop, clients_tx))?
        };
        let send = {
            let frames = frames.clone();
            let stop = stop.clone();
            thread::Builder::new()
                .name("preview-send".into())
                .spawn(move || send_loop(&frames, &stop, clients_rx))?
        };
        Ok(Self {
            port,
            token,
            frames,
            stop,
            threads: vec![accept, send],
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn url(&self) -> String {
        format!("ws://127.0.0.1:{}/?token={}", self.port, self.token)
    }

    /// Queues a frame; it replaces any frame that was not sent yet.
    pub fn send(&self, message: Vec<u8>) {
        self.frames.put(Bytes::from(message));
    }

    /// Frames replaced before they could be sent.
    pub fn dropped(&self) -> u64 {
        self.frames.dropped()
    }
}

impl Drop for PreviewServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.frames.close();
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

fn accept_loop(
    listener: TcpListener,
    token: &str,
    stop: &AtomicBool,
    clients: mpsc::Sender<WebSocket<TcpStream>>,
) {
    for stream in listener.incoming() {
        if stop.load(Ordering::SeqCst) {
            return;
        }
        let Ok(stream) = stream else { continue };
        if stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT)).is_err()
            || stream.set_write_timeout(Some(WRITE_TIMEOUT)).is_err()
        {
            continue;
        }
        // tungstenite's handshake callback fixes the error type.
        #[allow(clippy::result_large_err)]
        let check = |request: &Request, response: Response| match query_token(request) {
            Some(given) if same(given, token) => Ok(response),
            _ => Err(reject()),
        };
        match tungstenite::accept_hdr(stream, check) {
            Ok(socket) => {
                if clients.send(socket).is_err() {
                    return;
                }
            }
            Err(e) => log::debug!("preview client rejected: {e}"),
        }
    }
}

fn send_loop(
    frames: &Latest<Bytes>,
    stop: &AtomicBool,
    clients: mpsc::Receiver<WebSocket<TcpStream>>,
) {
    let mut client: Option<WebSocket<TcpStream>> = None;
    let mut last: Option<Bytes> = None;
    while !stop.load(Ordering::SeqCst) {
        while let Ok(socket) = clients.try_recv() {
            if let Some(mut old) = client.replace(socket) {
                let _ = old.close(None);
            }
            if let Some(frame) = &last {
                deliver(&mut client, frame.clone());
            }
        }
        if let Some(frame) = frames.wait(POLL) {
            last = Some(frame.clone());
            deliver(&mut client, frame);
        }
    }
}

fn deliver(client: &mut Option<WebSocket<TcpStream>>, frame: Bytes) {
    if let Some(socket) = client
        && let Err(e) = socket.send(Message::Binary(frame))
    {
        log::debug!("preview client dropped: {e}");
        *client = None;
    }
}

#[cfg(test)]
mod tests {
    use tungstenite::{Error, client::IntoClientRequest};

    use super::*;

    fn connect(
        url: &str,
    ) -> Result<WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>, Error> {
        let config = tungstenite::protocol::WebSocketConfig::default()
            .max_message_size(Some(64 << 20))
            .max_frame_size(Some(64 << 20));
        tungstenite::client::connect_with_config(url.into_client_request()?, Some(config), 0)
            .map(|(socket, _)| socket)
    }

    fn next_binary(
        socket: &mut WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>,
    ) -> Vec<u8> {
        loop {
            if let Message::Binary(bytes) = socket.read().unwrap() {
                return bytes.to_vec();
            }
        }
    }

    #[test]
    fn listens_on_loopback_with_a_long_token() {
        let server = PreviewServer::start().unwrap();
        assert_ne!(server.port(), 0);
        assert_eq!(server.token().len(), 48);
        assert!(server.url().starts_with("ws://127.0.0.1:"));
        let other = PreviewServer::start().unwrap();
        assert_ne!(server.token(), other.token());
    }

    #[test]
    fn rejects_clients_without_the_token() {
        let server = PreviewServer::start().unwrap();
        let base = format!("ws://127.0.0.1:{}/", server.port());
        for url in [
            base.clone(),
            format!("{base}?token="),
            format!("{base}?token=wrong"),
            format!("{base}?token={}x", server.token()),
        ] {
            match connect(&url) {
                Err(Error::Http(response)) => assert_eq!(response.status(), 403, "{url}"),
                other => panic!("{url} was not rejected: {:?}", other.map(|_| ())),
            }
        }
        assert!(connect(&server.url()).is_ok());
    }

    #[test]
    fn frames_queued_while_sending_are_dropped_except_the_newest() {
        let server = PreviewServer::start().unwrap();
        let mut client = connect(&server.url()).unwrap();
        // Larger than the socket buffers, so the sender blocks until the client reads.
        server.send(vec![0; 32 << 20]);
        thread::sleep(Duration::from_millis(200));
        for value in 1..=100u8 {
            server.send(vec![value]);
        }
        assert_eq!(server.dropped(), 99);
        assert_eq!(next_binary(&mut client).len(), 32 << 20);
        assert_eq!(next_binary(&mut client), vec![100]);
    }

    #[test]
    fn a_new_client_gets_the_last_frame() {
        let server = PreviewServer::start().unwrap();
        let mut first = connect(&server.url()).unwrap();
        server.send(vec![7, 7]);
        assert_eq!(next_binary(&mut first), vec![7, 7]);
        let mut second = connect(&server.url()).unwrap();
        assert_eq!(next_binary(&mut second), vec![7, 7]);
    }
}
