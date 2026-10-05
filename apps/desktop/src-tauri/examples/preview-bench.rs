//! Measures the editor preview through its WebSocket: how long a seek takes to
//! arrive as a frame, and the frame rate during playback.
//!
//! usage: preview-bench BUNDLE

use std::{
    net::TcpStream,
    path::PathBuf,
    process::exit,
    time::{Duration, Instant},
};

use recast_desktop_lib::editor::{EditorSession, protocol::FrameHeader};
use tungstenite::{Message, WebSocket, protocol::WebSocketConfig, stream::MaybeTlsStream};

type Socket = WebSocket<MaybeTlsStream<TcpStream>>;

fn next_frame(socket: &mut Socket) -> (FrameHeader, Instant) {
    loop {
        match socket.read() {
            Ok(Message::Binary(bytes)) => {
                let header = FrameHeader::decode(&bytes).expect("frame header");
                assert_eq!(bytes.len(), header.message_len());
                return (header, Instant::now());
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("preview stream closed: {e}");
                exit(1);
            }
        }
    }
}

fn wait_for(socket: &mut Socket, t_ms: f64) -> Instant {
    loop {
        let (header, at) = next_frame(socket);
        if (header.t_ms - t_ms).abs() < 1e-6 {
            return at;
        }
    }
}

fn summary(mut values: Vec<f64>) -> serde_json::Value {
    values.sort_by(f64::total_cmp);
    let at = |q: f64| values[((values.len() - 1) as f64 * q).round() as usize];
    serde_json::json!({
        "count": values.len(),
        "medianMs": at(0.5),
        "p90Ms": at(0.9),
        "maxMs": at(1.0),
    })
}

fn main() {
    let Some(path) = std::env::args().nth(1).map(PathBuf::from) else {
        eprintln!("usage: preview-bench BUNDLE");
        exit(2);
    };
    let session = EditorSession::open(&path).unwrap_or_else(|e| {
        eprintln!("cannot open {}: {e}", path.display());
        exit(1);
    });
    let init = session.init();
    let duration = init.duration_ms;
    let config = WebSocketConfig::default()
        .max_message_size(Some(64 << 20))
        .max_frame_size(Some(64 << 20));
    let (mut socket, _) =
        tungstenite::client::connect_with_config(init.preview_url.as_str(), Some(config), 0)
            .expect("connect to the preview");

    // A Retina-sized preview area; frames are capped at 1280 px.
    session.resize(2400, 1500);
    let (first, _) = next_frame(&mut socket);
    let mut seq = 0;
    let mut seek = |session: &EditorSession, socket: &mut Socket, t_ms: f64| {
        seq += 1;
        let started = Instant::now();
        session.seek(t_ms, seq);
        wait_for(socket, t_ms).duration_since(started).as_secs_f64() * 1000.0
    };

    let mut random = 0x2545_f491_u64;
    let mut jumps = Vec::new();
    for _ in 0..20 {
        random = random
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        let t = (random >> 11) as f64 / (1u64 << 53) as f64 * (duration - 100.0);
        jumps.push(seek(&session, &mut socket, t.round()));
    }
    let mut steps = Vec::new();
    let start = duration * 0.25;
    for i in 0..60 {
        steps.push(seek(
            &session,
            &mut socket,
            start + i as f64 * 1000.0 / 60.0,
        ));
    }
    let mut back = Vec::new();
    for i in 0..20 {
        back.push(seek(
            &session,
            &mut socket,
            start - i as f64 * 1000.0 / 60.0,
        ));
    }

    seek(&session, &mut socket, 0.0);
    session.play();
    let mut frames = 0;
    let mut times = Vec::new();
    let mut until = None;
    while until.is_none_or(|u: Instant| Instant::now() < u) {
        let (header, at) = next_frame(&mut socket);
        if !header.playing {
            continue;
        }
        let until = *until.get_or_insert(at + Duration::from_secs(3));
        if at < until {
            frames += 1;
            times.push(header.t_ms);
        }
    }
    session.pause();
    let forward = times.windows(2).all(|w| w[1] >= w[0]);
    let advanced = times.last().copied().unwrap_or(0.0) - times.first().copied().unwrap_or(0.0);

    let stats = session.stats();
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "bundle": init.bundle_path,
            "durationMs": duration,
            "frameSize": [first.width, first.height],
            "seekRandom": summary(jumps),
            "seekForwardFrame": summary(steps),
            "seekBackwardFrame": summary(back),
            "playback": {
                "framesPerSecond": frames as f64 / 3.0,
                "mediaAdvancedMs": advanced,
                "monotonic": forward,
            },
            "renderMsAverage": stats.render_ms,
            "droppedFrames": stats.dropped,
            "audioOutput": stats.audio_output,
        }))
        .expect("json")
    );
}
