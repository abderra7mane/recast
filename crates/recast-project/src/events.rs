use std::{
    fs::{File, OpenOptions},
    io::{BufWriter, Read, Write},
    path::Path,
};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::Result;

pub const EVENTS_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum MouseButton {
    Left,
    Right,
    Other,
}

/// Positions are global display points (origin at the top-left of the main display).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum EventKind {
    Move {
        x: f64,
        y: f64,
    },
    Drag {
        x: f64,
        y: f64,
        button: MouseButton,
    },
    #[serde(rename_all = "camelCase")]
    Down {
        x: f64,
        y: f64,
        button: MouseButton,
        click_count: u32,
    },
    Up {
        x: f64,
        y: f64,
        button: MouseButton,
    },
    Scroll {
        x: f64,
        y: f64,
        dx: f64,
        dy: f64,
    },
    Cursor {
        shape: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InputEvent {
    /// Milliseconds since the first video frame.
    pub t_ms: f64,
    pub kind: EventKind,
}

/// Hotspot and size are in points; the PNG is rendered at `scale` pixels per point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CursorShape {
    pub id: u32,
    pub file: String,
    pub hash: String,
    pub hotspot_x: f64,
    pub hotspot_y: f64,
    pub width: f64,
    pub height: f64,
    pub scale: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EventLog {
    pub version: u32,
    pub cursor_shapes: Vec<CursorShape>,
    pub events: Vec<InputEvent>,
}

impl Default for EventLog {
    fn default() -> Self {
        Self {
            version: EVENTS_VERSION,
            cursor_shapes: Vec::new(),
            events: Vec::new(),
        }
    }
}

impl EventLog {
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        Ok(rmp_serde::to_vec_named(self)?)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Ok(rmp_serde::from_slice(bytes)?)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Track {
    Video,
    SystemAudio,
    Mic,
}

/// One entry of the append-only log written while recording. Times are host-clock nanoseconds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LogRecord {
    TrackStarted { track: Track, host_ns: u64 },
    Input { host_ns: u64, kind: EventKind },
    CursorShape(CursorShape),
}

/// Writes length-prefixed msgpack records so a crash loses at most the record being written.
pub struct RecordLogWriter {
    out: BufWriter<File>,
}

impl RecordLogWriter {
    pub fn create(path: &Path) -> Result<Self> {
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self {
            out: BufWriter::new(file),
        })
    }

    pub fn append(&mut self, record: &LogRecord) -> Result<()> {
        let bytes = rmp_serde::to_vec_named(record)?;
        self.out.write_all(&(bytes.len() as u32).to_le_bytes())?;
        self.out.write_all(&bytes)?;
        self.out.flush()?;
        Ok(())
    }
}

/// Reads every complete record; a truncated or corrupt tail is ignored.
pub fn read_record_log(path: &Path) -> Result<Vec<LogRecord>> {
    let mut bytes = Vec::new();
    File::open(path)?.read_to_end(&mut bytes)?;
    Ok(parse_records(&bytes))
}

fn parse_records(bytes: &[u8]) -> Vec<LogRecord> {
    let mut records = Vec::new();
    let mut pos = 0usize;
    while let Some(len_bytes) = bytes.get(pos..pos + 4) {
        let len = u32::from_le_bytes(len_bytes.try_into().expect("4 bytes")) as usize;
        let Some(body) = bytes.get(pos + 4..pos + 4 + len) else {
            break;
        };
        let Ok(record) = rmp_serde::from_slice(body) else {
            break;
        };
        records.push(record);
        pos += 4 + len;
    }
    records
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TrackStarts {
    pub video: Option<u64>,
    pub system_audio: Option<u64>,
    pub mic: Option<u64>,
}

impl TrackStarts {
    pub fn from_records(records: &[LogRecord]) -> Self {
        let mut starts = Self::default();
        for record in records {
            if let LogRecord::TrackStarted { track, host_ns } = record {
                let slot = match track {
                    Track::Video => &mut starts.video,
                    Track::SystemAudio => &mut starts.system_audio,
                    Track::Mic => &mut starts.mic,
                };
                slot.get_or_insert(*host_ns);
            }
        }
        starts
    }
}

pub fn ns_delta_ms(from_ns: u64, to_ns: u64) -> f64 {
    (to_ns as i128 - from_ns as i128) as f64 / 1_000_000.0
}

/// Builds the final event log with times relative to `video_start_ns`.
/// Events before the first frame are dropped, except the last known cursor
/// position and shape, which are kept at time zero.
pub fn compile_event_log(records: &[LogRecord], video_start_ns: u64) -> EventLog {
    let mut log = EventLog::default();
    let mut last_position: Option<(f64, f64)> = None;
    let mut last_shape: Option<u32> = None;
    let mut events = Vec::new();

    for record in records {
        match record {
            LogRecord::CursorShape(shape) => log.cursor_shapes.push(shape.clone()),
            LogRecord::Input { host_ns, kind } if *host_ns < video_start_ns => match kind {
                EventKind::Move { x, y } | EventKind::Drag { x, y, .. } => {
                    last_position = Some((*x, *y))
                }
                EventKind::Cursor { shape } => last_shape = Some(*shape),
                _ => {}
            },
            LogRecord::Input { host_ns, kind } => events.push(InputEvent {
                t_ms: ns_delta_ms(video_start_ns, *host_ns),
                kind: kind.clone(),
            }),
            LogRecord::TrackStarted { .. } => {}
        }
    }

    if let Some(shape) = last_shape {
        log.events.push(InputEvent {
            t_ms: 0.0,
            kind: EventKind::Cursor { shape },
        });
    }
    if let Some((x, y)) = last_position {
        log.events.push(InputEvent {
            t_ms: 0.0,
            kind: EventKind::Move { x, y },
        });
    }
    events.sort_by(|a, b| a.t_ms.total_cmp(&b.t_ms));
    log.events.extend(events);
    log
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(id: u32) -> CursorShape {
        CursorShape {
            id,
            file: format!("cursors/{id}.png"),
            hash: format!("{id:016x}"),
            hotspot_x: 4.0,
            hotspot_y: 4.5,
            width: 17.0,
            height: 23.0,
            scale: 2.0,
        }
    }

    fn sample_log() -> EventLog {
        EventLog {
            version: EVENTS_VERSION,
            cursor_shapes: vec![shape(0), shape(1)],
            events: vec![
                InputEvent {
                    t_ms: 0.0,
                    kind: EventKind::Cursor { shape: 0 },
                },
                InputEvent {
                    t_ms: 1.25,
                    kind: EventKind::Move { x: 10.5, y: -3.0 },
                },
                InputEvent {
                    t_ms: 16.0,
                    kind: EventKind::Down {
                        x: 11.0,
                        y: 12.0,
                        button: MouseButton::Left,
                        click_count: 2,
                    },
                },
                InputEvent {
                    t_ms: 20.0,
                    kind: EventKind::Drag {
                        x: 12.0,
                        y: 13.0,
                        button: MouseButton::Left,
                    },
                },
                InputEvent {
                    t_ms: 40.0,
                    kind: EventKind::Up {
                        x: 12.0,
                        y: 13.0,
                        button: MouseButton::Right,
                    },
                },
                InputEvent {
                    t_ms: 41.0,
                    kind: EventKind::Scroll {
                        x: 1.0,
                        y: 2.0,
                        dx: 0.0,
                        dy: -12.5,
                    },
                },
                InputEvent {
                    t_ms: 50.0,
                    kind: EventKind::Cursor { shape: 1 },
                },
            ],
        }
    }

    #[test]
    fn event_log_msgpack_round_trip() {
        let log = sample_log();
        let bytes = log.to_bytes().unwrap();
        assert_eq!(EventLog::from_bytes(&bytes).unwrap(), log);
    }

    #[test]
    fn event_kind_uses_type_tag() {
        let json = serde_json::to_value(EventKind::Down {
            x: 1.0,
            y: 2.0,
            button: MouseButton::Left,
            click_count: 1,
        })
        .unwrap();
        assert_eq!(json["type"], "down");
        assert_eq!(json["button"], "left");
        assert_eq!(json["clickCount"], 1);
    }

    #[test]
    fn record_log_round_trip_and_truncated_tail() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.log");
        let records = vec![
            LogRecord::TrackStarted {
                track: Track::Video,
                host_ns: 1_000,
            },
            LogRecord::CursorShape(shape(3)),
            LogRecord::Input {
                host_ns: 2_000,
                kind: EventKind::Move { x: 1.0, y: 2.0 },
            },
        ];
        let mut writer = RecordLogWriter::create(&path).unwrap();
        for record in &records {
            writer.append(record).unwrap();
        }
        drop(writer);
        assert_eq!(read_record_log(&path).unwrap(), records);

        let mut bytes = std::fs::read(&path).unwrap();
        bytes.truncate(bytes.len() - 3);
        std::fs::write(&path, &bytes).unwrap();
        assert_eq!(read_record_log(&path).unwrap(), records[..2]);
    }

    #[test]
    fn compile_aligns_to_first_frame() {
        let records = vec![
            LogRecord::Input {
                host_ns: 500_000,
                kind: EventKind::Cursor { shape: 7 },
            },
            LogRecord::Input {
                host_ns: 900_000,
                kind: EventKind::Move { x: 5.0, y: 6.0 },
            },
            LogRecord::Input {
                host_ns: 950_000,
                kind: EventKind::Down {
                    x: 5.0,
                    y: 6.0,
                    button: MouseButton::Left,
                    click_count: 1,
                },
            },
            LogRecord::TrackStarted {
                track: Track::Video,
                host_ns: 1_000_000,
            },
            LogRecord::TrackStarted {
                track: Track::Mic,
                host_ns: 1_500_000,
            },
            LogRecord::Input {
                host_ns: 4_000_000,
                kind: EventKind::Move { x: 7.0, y: 8.0 },
            },
            LogRecord::Input {
                host_ns: 3_000_000,
                kind: EventKind::Up {
                    x: 5.0,
                    y: 6.0,
                    button: MouseButton::Left,
                },
            },
        ];
        let starts = TrackStarts::from_records(&records);
        assert_eq!(starts.video, Some(1_000_000));
        assert_eq!(starts.mic, Some(1_500_000));
        assert_eq!(starts.system_audio, None);

        let log = compile_event_log(&records, starts.video.unwrap());
        let times: Vec<f64> = log.events.iter().map(|e| e.t_ms).collect();
        assert_eq!(times, vec![0.0, 0.0, 2.0, 3.0]);
        assert_eq!(log.events[0].kind, EventKind::Cursor { shape: 7 });
        assert_eq!(log.events[1].kind, EventKind::Move { x: 5.0, y: 6.0 });
        assert!(matches!(log.events[2].kind, EventKind::Up { .. }));
        assert_eq!(ns_delta_ms(1_000_000, 500_000), -0.5);
    }
}
