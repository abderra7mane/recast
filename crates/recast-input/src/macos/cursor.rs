use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use objc2::{AllocAnyThread, rc::autoreleasepool};
use objc2_app_kit::{
    NSBitmapImageFileType, NSBitmapImageRep, NSCursor, NSDeviceRGBColorSpace, NSGraphicsContext,
    NSImageInterpolation,
};
use objc2_foundation::{NSDictionary, NSPoint, NSRect, NSSize};
use recast_project::{CursorShape, EventKind};

use super::now_host_ns;
use crate::{InputRecord, InputSink};

const INTERVAL: Duration = Duration::from_millis(33);
const MAX_SCALE: f64 = 4.0;

pub(super) struct CursorSampler {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl CursorSampler {
    pub(super) fn start(dir: PathBuf, relative_dir: String, sink: InputSink) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = thread::Builder::new()
            .name("recast-cursor".into())
            .spawn(move || {
                let mut state = SamplerState {
                    dir,
                    relative_dir,
                    sink,
                    known: HashMap::new(),
                    current: None,
                };
                while !flag.load(Ordering::Acquire) {
                    let started = Instant::now();
                    autoreleasepool(|_| state.sample());
                    thread::sleep(INTERVAL.saturating_sub(started.elapsed()));
                }
            })
            .ok();
        Self { stop, thread }
    }

    pub(super) fn stop(mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Snapshot {
    hash: u64,
    hotspot: NSPoint,
    size: NSSize,
    scale: f64,
    rep: objc2::rc::Retained<NSBitmapImageRep>,
}

struct SamplerState {
    dir: PathBuf,
    relative_dir: String,
    sink: InputSink,
    known: HashMap<u64, u32>,
    current: Option<u32>,
}

impl SamplerState {
    fn sample(&mut self) {
        #[allow(deprecated)]
        let Some(cursor) = NSCursor::currentSystemCursor() else {
            return;
        };
        if let Some(snapshot) = snapshot(&cursor) {
            self.observe(&snapshot);
        }
    }

    fn observe(&mut self, snapshot: &Snapshot) {
        let id = match self.known.get(&snapshot.hash) {
            Some(id) => *id,
            None => match self.store(snapshot) {
                Some(id) => id,
                None => return,
            },
        };
        if self.current != Some(id) {
            self.current = Some(id);
            (self.sink)(InputRecord::Event {
                host_ns: now_host_ns(),
                kind: EventKind::Cursor { shape: id },
            });
        }
    }

    fn store(&mut self, snapshot: &Snapshot) -> Option<u32> {
        let id = self.known.len() as u32;
        // SAFETY: an empty properties dictionary is valid for PNG output.
        let png = unsafe {
            snapshot.rep.representationUsingType_properties(
                NSBitmapImageFileType::PNG,
                &NSDictionary::new(),
            )
        }?;
        let name = format!("{id}.png");
        std::fs::write(self.dir.join(&name), png.to_vec()).ok()?;
        self.known.insert(snapshot.hash, id);
        (self.sink)(InputRecord::Shape(CursorShape {
            id,
            file: format!("{}/{name}", self.relative_dir),
            hash: format!("{:016x}", snapshot.hash),
            hotspot_x: snapshot.hotspot.x,
            hotspot_y: snapshot.hotspot.y,
            width: snapshot.size.width,
            height: snapshot.size.height,
            scale: snapshot.scale,
        }));
        Some(id)
    }
}

/// Renders a cursor into an RGBA bitmap and hashes its pixels.
fn snapshot(cursor: &NSCursor) -> Option<Snapshot> {
    let image = cursor.image();
    let hotspot = cursor.hotSpot();
    let size = image.size();
    if size.width <= 0.0 || size.height <= 0.0 {
        return None;
    }
    let best = image
        .representations()
        .iter()
        .map(|rep| rep.pixelsWide() as f64 / size.width)
        .fold(1.0, f64::max);
    let scale = best.clamp(2.0, MAX_SCALE);
    let pixels_wide = (size.width * scale).round() as isize;
    let pixels_high = (size.height * scale).round() as isize;

    // SAFETY: null planes make AppKit allocate the bitmap; the arguments describe 8-bit RGBA.
    let rep = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            std::ptr::null_mut(),
            pixels_wide,
            pixels_high,
            8,
            4,
            true,
            false,
            NSDeviceRGBColorSpace,
            0,
            0,
        )
    }?;
    rep.setSize(size);
    let ctx = NSGraphicsContext::graphicsContextWithBitmapImageRep(&rep)?;
    NSGraphicsContext::saveGraphicsState_class();
    NSGraphicsContext::setCurrentContext(Some(&ctx));
    ctx.setImageInterpolation(NSImageInterpolation::High);
    image.drawInRect(NSRect::new(NSPoint::new(0.0, 0.0), size));
    ctx.flushGraphics();
    NSGraphicsContext::restoreGraphicsState_class();

    let len = rep.bytesPerRow() as usize * rep.pixelsHigh() as usize;
    let data = rep.bitmapData();
    if data.is_null() {
        return None;
    }
    // SAFETY: the bitmap owns `bytesPerRow * pixelsHigh` bytes and stays alive while hashing.
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hotspot.x.to_bits().hash(&mut hasher);
    hotspot.y.to_bits().hash(&mut hasher);
    size.width.to_bits().hash(&mut hasher);
    size.height.to_bits().hash(&mut hasher);

    Some(Snapshot {
        hash: hasher.finish(),
        hotspot,
        size,
        scale,
        rep,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use objc2_app_kit::NSImage;

    use super::*;

    fn state(dir: &std::path::Path) -> (SamplerState, Arc<Mutex<Vec<InputRecord>>>) {
        let records = Arc::new(Mutex::new(Vec::new()));
        let sink_records = records.clone();
        let state = SamplerState {
            dir: dir.to_path_buf(),
            relative_dir: "cursors".into(),
            sink: Arc::new(move |r| sink_records.lock().unwrap().push(r)),
            known: HashMap::new(),
            current: None,
        };
        (state, records)
    }

    /// A 16×16 cursor filled with one gray level.
    fn solid_cursor(level: u8) -> objc2::rc::Retained<NSCursor> {
        // SAFETY: null planes make AppKit allocate an 8-bit RGBA bitmap.
        let rep = unsafe {
            NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
                NSBitmapImageRep::alloc(),
                std::ptr::null_mut(),
                16,
                16,
                8,
                4,
                true,
                false,
                NSDeviceRGBColorSpace,
                0,
                0,
            )
        }
        .unwrap();
        let len = rep.bytesPerRow() as usize * 16;
        // SAFETY: the bitmap owns `bytesPerRow * 16` bytes.
        unsafe { std::slice::from_raw_parts_mut(rep.bitmapData(), len) }.fill(level);
        let image = NSImage::initWithSize(NSImage::alloc(), NSSize::new(16.0, 16.0));
        image.addRepresentation(&rep);
        NSCursor::initWithImage_hotSpot(NSCursor::alloc(), &image, NSPoint::new(1.0, 1.0))
    }

    #[test]
    fn stores_each_shape_once_and_records_changes() {
        let dir = tempfile::tempdir().unwrap();
        let (mut state, records) = state(dir.path());
        let arrow = solid_cursor(40);
        let ibeam = solid_cursor(200);
        for cursor in [&arrow, &arrow, &ibeam, &arrow] {
            autoreleasepool(|_| state.observe(&snapshot(cursor).expect("snapshot")));
        }

        let records = records.lock().unwrap();
        let shapes: Vec<&CursorShape> = records
            .iter()
            .filter_map(|r| match r {
                InputRecord::Shape(s) => Some(s),
                _ => None,
            })
            .collect();
        assert_eq!(shapes.len(), 2, "{records:?}");
        assert_eq!(shapes[0].file, "cursors/0.png");
        assert_eq!(shapes[1].file, "cursors/1.png");
        assert!(shapes.iter().all(|s| s.scale >= 2.0));
        assert_ne!(shapes[0].hash, shapes[1].hash);
        for name in ["0.png", "1.png"] {
            let png = std::fs::read(dir.path().join(name)).unwrap();
            assert_eq!(&png[1..4], b"PNG");
        }
        let changes: Vec<u32> = records
            .iter()
            .filter_map(|r| match r {
                InputRecord::Event {
                    kind: EventKind::Cursor { shape },
                    ..
                } => Some(*shape),
                _ => None,
            })
            .collect();
        assert_eq!(changes, vec![0, 1, 0]);
    }

    /// Needs a logged-in GUI session; fails rather than skips without one.
    #[test]
    fn reads_the_system_cursor() {
        #[allow(deprecated)]
        let cursor = NSCursor::currentSystemCursor().expect("no system cursor in this session");
        let shot = autoreleasepool(|_| snapshot(&cursor)).expect("snapshot");
        assert!(shot.size.width > 0.0 && shot.scale >= 2.0);
    }
}
