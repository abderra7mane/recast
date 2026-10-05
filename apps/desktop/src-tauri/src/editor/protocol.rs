//! Preview frames sent over the WebSocket: a 32-byte little-endian header followed
//! by the NV12 planes (Y, then interleaved UV at half size).
//!
//! | bytes  | field                         |
//! |--------|-------------------------------|
//! | 0..4   | magic `RCF1`                  |
//! | 4..8   | width (u32)                   |
//! | 8..12  | height (u32)                  |
//! | 12..16 | flags (u32, bit 0: playing)   |
//! | 16..24 | sequence (u64)                |
//! | 24..32 | frame time in ms (f64)        |

use std::{
    sync::{Condvar, Mutex},
    time::Duration,
};

pub const MAGIC: [u8; 4] = *b"RCF1";
pub const HEADER_LEN: usize = 32;
const PLAYING: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameHeader {
    pub width: u32,
    pub height: u32,
    pub playing: bool,
    /// Increases with every frame sent.
    pub seq: u64,
    /// Time on the recording's timeline.
    pub t_ms: f64,
}

impl FrameHeader {
    pub fn encode(&self) -> [u8; HEADER_LEN] {
        let mut out = [0; HEADER_LEN];
        out[0..4].copy_from_slice(&MAGIC);
        out[4..8].copy_from_slice(&self.width.to_le_bytes());
        out[8..12].copy_from_slice(&self.height.to_le_bytes());
        let flags = if self.playing { PLAYING } else { 0 };
        out[12..16].copy_from_slice(&flags.to_le_bytes());
        out[16..24].copy_from_slice(&self.seq.to_le_bytes());
        out[24..32].copy_from_slice(&self.t_ms.to_le_bytes());
        out
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let header = bytes.get(..HEADER_LEN)?;
        if header[0..4] != MAGIC {
            return None;
        }
        let u32_at = |i: usize| u32::from_le_bytes(header[i..i + 4].try_into().expect("4 bytes"));
        Some(Self {
            width: u32_at(4),
            height: u32_at(8),
            playing: u32_at(12) & PLAYING != 0,
            seq: u64::from_le_bytes(header[16..24].try_into().expect("8 bytes")),
            t_ms: f64::from_le_bytes(header[24..32].try_into().expect("8 bytes")),
        })
    }

    /// Length of the whole message for this header.
    pub fn message_len(&self) -> usize {
        HEADER_LEN + recast_render::nv12::len(self.width, self.height)
    }
}

/// A slot holding only the newest value: putting a value replaces one that was not
/// taken yet, so a slow reader always gets the latest and never a backlog.
pub struct Latest<T> {
    state: Mutex<LatestState<T>>,
    changed: Condvar,
}

struct LatestState<T> {
    value: Option<T>,
    dropped: u64,
    closed: bool,
}

impl<T> Default for Latest<T> {
    fn default() -> Self {
        Self {
            state: Mutex::new(LatestState {
                value: None,
                dropped: 0,
                closed: false,
            }),
            changed: Condvar::new(),
        }
    }
}

impl<T> Latest<T> {
    pub fn put(&self, value: T) {
        let mut state = self.state.lock().expect("latest lock");
        if state.value.replace(value).is_some() {
            state.dropped += 1;
        }
        self.changed.notify_all();
    }

    pub fn take(&self) -> Option<T> {
        self.state.lock().expect("latest lock").value.take()
    }

    /// Waits up to `timeout` for a value. Returns `None` on timeout or once closed.
    pub fn wait(&self, timeout: Duration) -> Option<T> {
        let state = self.state.lock().expect("latest lock");
        let (mut state, _) = self
            .changed
            .wait_timeout_while(state, timeout, |s| s.value.is_none() && !s.closed)
            .expect("latest lock");
        state.value.take()
    }

    /// How many values were replaced before anyone took them.
    pub fn dropped(&self) -> u64 {
        self.state.lock().expect("latest lock").dropped
    }

    pub fn close(&self) {
        self.state.lock().expect("latest lock").closed = true;
        self.changed.notify_all();
    }

    pub fn is_closed(&self) -> bool {
        self.state.lock().expect("latest lock").closed
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, thread, time::Instant};

    use super::*;

    #[test]
    fn header_round_trip() {
        let header = FrameHeader {
            width: 1280,
            height: 800,
            playing: true,
            seq: 42,
            t_ms: 1234.5,
        };
        let bytes = header.encode();
        assert_eq!(&bytes[0..4], b"RCF1");
        assert_eq!(&bytes[4..8], &1280u32.to_le_bytes());
        assert_eq!(&bytes[12..16], &1u32.to_le_bytes());
        assert_eq!(FrameHeader::decode(&bytes), Some(header));
        assert_eq!(header.message_len(), 32 + 1280 * 800 * 3 / 2);

        let paused = FrameHeader {
            playing: false,
            ..header
        };
        assert_eq!(FrameHeader::decode(&paused.encode()), Some(paused));
    }

    #[test]
    fn header_rejects_bad_input() {
        assert_eq!(FrameHeader::decode(&[0; 31]), None);
        let mut bytes = FrameHeader {
            width: 2,
            height: 2,
            playing: false,
            seq: 0,
            t_ms: 0.0,
        }
        .encode();
        bytes[0] = b'X';
        assert_eq!(FrameHeader::decode(&bytes), None);
    }

    #[test]
    fn latest_keeps_only_the_newest_value() {
        let slot = Latest::default();
        slot.put(1);
        slot.put(2);
        slot.put(3);
        assert_eq!(slot.take(), Some(3));
        assert_eq!(slot.take(), None);
        assert_eq!(slot.dropped(), 2);
    }

    #[test]
    fn latest_wakes_a_waiting_reader() {
        let slot = Arc::new(Latest::default());
        let writer = {
            let slot = slot.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(20));
                slot.put("frame");
            })
        };
        let started = Instant::now();
        assert_eq!(slot.wait(Duration::from_secs(5)), Some("frame"));
        assert!(started.elapsed() < Duration::from_secs(1));
        writer.join().unwrap();
        assert_eq!(slot.wait(Duration::from_millis(10)), None);
        slot.close();
        assert!(slot.is_closed());
        assert_eq!(slot.wait(Duration::from_secs(5)), None);
    }
}
