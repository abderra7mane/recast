//! Images of annotations and where blur and pixelate average the recording.
//!
//! The editor draws each annotation that has a look of its own (everything but blur and
//! pixelate) into an image with the same Canvas 2D code the screenshot editor uses, and
//! sends it here. Images are keyed by [`AnnotationShape::look_key`], so moving an
//! annotation keeps its image. They are saved in the bundle's `annotations` folder, so an
//! export draws exactly what the preview showed.

use std::{
    collections::{HashMap, HashSet},
    fmt,
    path::Path,
    sync::{Arc, Mutex},
};

use recast_project::{AnnotationShape, Annotations, Rect};
use serde::{Deserialize, Serialize};

use crate::{Error, Result, bitmap};

/// The folder in a bundle that holds annotation images.
pub const DIR: &str = "annotations";
const INDEX: &str = "index.json";
/// Largest side of an annotation image, in pixels.
pub const MAX_RASTER_SIDE: u32 = 8_192;
/// Bytes of annotation images kept in memory that the edits no longer use, for undo.
pub const SPARE_BYTES: usize = 256 << 20;
/// Largest number of cells along one side of a blurred or pixelated area; larger areas
/// get larger cells.
pub const MAX_CELLS: u32 = 512;

/// An annotation drawn as straight RGBA. Its top-left corner is `x, y` points from the
/// shape's anchor and it covers `width_pts × height_pts` points of the recorded area.
#[derive(Clone, PartialEq)]
pub struct Raster {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub x: f64,
    pub y: f64,
    pub width_pts: f64,
    pub height_pts: f64,
}

impl fmt::Debug for Raster {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Raster({}×{} px at {}, {} pt, {}×{} pt)",
            self.width, self.height, self.x, self.y, self.width_pts, self.height_pts
        )
    }
}

impl Raster {
    pub fn new(
        width: u32,
        height: u32,
        rgba: Vec<u8>,
        (x, y): (f64, f64),
        (width_pts, height_pts): (f64, f64),
    ) -> Result<Self> {
        let sides = 1..=MAX_RASTER_SIDE;
        if !sides.contains(&width) || !sides.contains(&height) {
            return Err(Error::Image(format!(
                "{width}×{height} is not a supported annotation size"
            )));
        }
        if rgba.len() != width as usize * height as usize * 4 {
            return Err(Error::Image(format!(
                "{} bytes is not a {width}×{height} RGBA image",
                rgba.len()
            )));
        }
        let sizes = [x, y, width_pts, height_pts];
        if sizes.iter().any(|v| !v.is_finite()) || width_pts <= 0.0 || height_pts <= 0.0 {
            return Err(Error::Image(
                "the annotation image has no valid place".into(),
            ));
        }
        Ok(Self {
            width,
            height,
            rgba,
            x,
            y,
            width_pts,
            height_pts,
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IndexEntry {
    key: String,
    file: String,
    x: f64,
    y: f64,
    width_pts: f64,
    height_pts: f64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Index {
    rasters: Vec<IndexEntry>,
}

#[derive(Default)]
struct Store {
    rasters: HashMap<String, (Arc<Raster>, u64)>,
    clock: u64,
}

/// Annotation images by look key, shared by the editor, the preview and autosave.
#[derive(Default)]
pub struct Rasters {
    store: Mutex<Store>,
}

impl fmt::Debug for Rasters {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Rasters({})", self.len())
    }
}

/// FNV-1a, for file names that stay the same across runs.
fn file_name(key: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in key.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}.png")
}

fn write_atomic(path: &Path, write: impl FnOnce(&Path) -> Result<()>) -> Result<()> {
    let tmp = path.with_extension("tmp");
    write(&tmp)?;
    std::fs::rename(&tmp, path).map_err(|e| Error::Image(format!("{}: {e}", path.display())))
}

impl Rasters {
    fn lock(&self) -> std::sync::MutexGuard<'_, Store> {
        self.store.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn len(&self) -> usize {
        self.lock().rasters.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get(&self, key: &str) -> Option<Arc<Raster>> {
        self.lock()
            .rasters
            .get(key)
            .map(|(raster, _)| raster.clone())
    }

    pub fn insert(&self, key: String, raster: Raster) -> Arc<Raster> {
        let mut store = self.lock();
        store.clock += 1;
        let raster = Arc::new(raster);
        let clock = store.clock;
        store.rasters.insert(key, (raster.clone(), clock));
        raster
    }

    /// Ids of the annotations whose image is missing, leaving out `hidden`.
    pub fn missing(&self, annotations: &Annotations, hidden: Option<&str>) -> Vec<String> {
        let store = self.lock();
        annotations
            .items
            .iter()
            .filter(|a| hidden != Some(a.id.as_str()))
            .filter(|a| {
                a.shape
                    .look_key()
                    .is_some_and(|key| !store.rasters.contains_key(&key))
            })
            .map(|a| a.id.clone())
            .collect()
    }

    /// Drops images `annotations` doesn't use, except the ones added last up to
    /// `spare_bytes`, which an undo may need again.
    pub fn trim(&self, annotations: &Annotations, spare_bytes: usize) {
        let used = used_keys(annotations);
        let mut store = self.lock();
        let mut unused: Vec<(u64, usize, String)> = store
            .rasters
            .iter()
            .filter(|(key, _)| !used.contains(key.as_str()))
            .map(|(key, (raster, clock))| (*clock, raster.rgba.len(), key.clone()))
            .collect();
        unused.sort_by_key(|entry| std::cmp::Reverse(entry.0));
        let mut kept = 0;
        for (_, bytes, key) in unused {
            kept += bytes;
            if kept > spare_bytes {
                store.rasters.remove(&key);
            }
        }
    }

    /// Loads the images saved in `dir`; a missing folder or file is skipped.
    pub fn load(dir: &Path) -> Self {
        let rasters = Self::default();
        let Ok(bytes) = std::fs::read(dir.join(INDEX)) else {
            return rasters;
        };
        let index: Index = match serde_json::from_slice(&bytes) {
            Ok(index) => index,
            Err(e) => {
                log::warn!("annotation index: {e}");
                return rasters;
            }
        };
        for entry in index.rasters {
            let loaded = bitmap::load_png(&dir.join(&entry.file)).and_then(|(w, h, rgba)| {
                Raster::new(
                    w,
                    h,
                    rgba,
                    (entry.x, entry.y),
                    (entry.width_pts, entry.height_pts),
                )
            });
            match loaded {
                Ok(raster) => {
                    rasters.insert(entry.key, raster);
                }
                Err(e) => log::warn!("annotation image {}: {e}", entry.file),
            }
        }
        rasters
    }

    /// Saves the images `annotations` uses in `dir` and removes the others.
    pub fn save(&self, dir: &Path, annotations: &Annotations) -> Result<()> {
        let io = |e: std::io::Error| Error::Image(format!("{}: {e}", dir.display()));
        let used = used_keys(annotations);
        if used.is_empty() && !dir.exists() {
            return Ok(());
        }
        std::fs::create_dir_all(dir).map_err(io)?;
        let mut index = Index::default();
        for key in &used {
            let Some(raster) = self.get(key) else {
                continue;
            };
            let file = file_name(key);
            let path = dir.join(&file);
            if !path.exists() {
                write_atomic(&path, |tmp| {
                    let png = bitmap::encode_png(raster.width, raster.height, &raster.rgba)?;
                    std::fs::write(tmp, png).map_err(io)
                })?;
            }
            index.rasters.push(IndexEntry {
                key: key.clone(),
                file,
                x: raster.x,
                y: raster.y,
                width_pts: raster.width_pts,
                height_pts: raster.height_pts,
            });
        }
        index.rasters.sort_by(|a, b| a.file.cmp(&b.file));
        let json = serde_json::to_vec_pretty(&index).map_err(|e| Error::Image(e.to_string()))?;
        write_atomic(&dir.join(INDEX), |tmp| {
            std::fs::write(tmp, json).map_err(io)
        })?;
        let kept: HashSet<&str> = index.rasters.iter().map(|e| e.file.as_str()).collect();
        for entry in std::fs::read_dir(dir).map_err(io)?.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.ends_with(".png") && !kept.contains(name.as_ref()) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
        Ok(())
    }
}

fn used_keys(annotations: &Annotations) -> HashSet<String> {
    annotations
        .items
        .iter()
        .filter_map(|a| a.shape.look_key())
        .collect()
}

/// Which video pixels a blurred or pixelated area averages: `rect` in whole texels of
/// the screen texture, averaged in `block × block` cells aligned to its corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellGrid {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub block: u32,
    pub columns: u32,
    pub rows: u32,
    pub pixelate: bool,
}

impl CellGrid {
    /// The cells of `shape` (blur or pixelate) on a `texture` of the `screen` (points).
    /// `None` for other shapes and areas outside the screen.
    pub fn new(shape: &AnnotationShape, screen: (f64, f64), texture: (u32, u32)) -> Option<Self> {
        let (rect, block, pixelate) = match shape {
            AnnotationShape::Blur { rect, block } => (rect, *block, false),
            AnnotationShape::Pixelate { rect, block } => (rect, *block, true),
            _ => return None,
        };
        let sx = texture.0 as f64 / screen.0.max(1e-9);
        let sy = texture.1 as f64 / screen.1.max(1e-9);
        let Rect {
            x,
            y,
            width,
            height,
        } = *rect;
        let left = (x * sx).floor().clamp(0.0, texture.0 as f64) as u32;
        let top = (y * sy).floor().clamp(0.0, texture.1 as f64) as u32;
        let right = ((x + width) * sx).ceil().clamp(0.0, texture.0 as f64) as u32;
        let bottom = ((y + height) * sy).ceil().clamp(0.0, texture.1 as f64) as u32;
        if right <= left || bottom <= top {
            return None;
        }
        let (width, height) = (right - left, bottom - top);
        let block = ((block * sx).round().max(2.0) as u32)
            .max(width.div_ceil(MAX_CELLS))
            .max(height.div_ceil(MAX_CELLS));
        Some(Self {
            x: left,
            y: top,
            width,
            height,
            block,
            columns: width.div_ceil(block),
            rows: height.div_ceil(block),
            pixelate,
        })
    }
}

/// Places grids of `columns × rows` cells side by side in rows `width` cells wide;
/// returns each grid's corner and the height used.
pub fn pack(grids: &[(u32, u32)], width: u32) -> (Vec<(u32, u32)>, u32) {
    let mut corners = Vec::with_capacity(grids.len());
    let (mut x, mut y, mut shelf) = (0, 0, 0);
    for &(columns, rows) in grids {
        if x > 0 && x + columns > width {
            x = 0;
            y += shelf;
            shelf = 0;
        }
        corners.push((x, y));
        x += columns;
        shelf = shelf.max(rows);
    }
    (corners, y + shelf)
}

#[cfg(test)]
mod tests {
    use recast_project::{Annotation, Color, Point};

    use super::*;

    fn rect_shape(width: f64) -> AnnotationShape {
        AnnotationShape::Rect {
            rect: Rect {
                x: 10.0,
                y: 10.0,
                width,
                height: 20.0,
            },
            color: Color::rgb(255, 0, 0),
            width: 2.0,
            fill: false,
        }
    }

    fn annotations(shapes: Vec<AnnotationShape>) -> Annotations {
        Annotations {
            tracks: 1,
            items: shapes
                .into_iter()
                .enumerate()
                .map(|(i, shape)| Annotation {
                    id: format!("a{i}"),
                    start_ms: 0.0,
                    end_ms: 1_000.0,
                    track: 0,
                    shape,
                })
                .collect(),
        }
    }

    fn raster(value: u8) -> Raster {
        Raster::new(2, 1, vec![value; 8], (-1.0, -1.5), (1.0, 0.5)).unwrap()
    }

    #[test]
    fn reports_missing_images_except_hidden_and_obscure() {
        let blur = AnnotationShape::Blur {
            rect: Rect {
                x: 0.0,
                y: 0.0,
                width: 5.0,
                height: 5.0,
            },
            block: 12.0,
        };
        let set = annotations(vec![rect_shape(40.0), rect_shape(50.0), blur]);
        let rasters = Rasters::default();
        assert_eq!(rasters.missing(&set, None), ["a0", "a1"]);
        rasters.insert(rect_shape(40.0).look_key().unwrap(), raster(1));
        assert_eq!(rasters.missing(&set, None), ["a1"]);
        assert!(rasters.missing(&set, Some("a1")).is_empty());
    }

    #[test]
    fn rejects_images_that_do_not_match_their_size() {
        assert!(Raster::new(2, 2, vec![0; 15], (0.0, 0.0), (1.0, 1.0)).is_err());
        assert!(Raster::new(0, 2, vec![], (0.0, 0.0), (1.0, 1.0)).is_err());
        assert!(Raster::new(1, 1, vec![0; 4], (f64::NAN, 0.0), (1.0, 1.0)).is_err());
        assert!(Raster::new(1, 1, vec![0; 4], (0.0, 0.0), (0.0, 1.0)).is_err());
    }

    #[test]
    fn trim_keeps_used_and_recent_images_up_to_a_size() {
        let rasters = Rasters::default();
        for width in [1.0, 2.0, 3.0, 4.0] {
            rasters.insert(rect_shape(width).look_key().unwrap(), raster(0));
        }
        // Each image is 8 bytes: room for the two added last.
        rasters.trim(&annotations(vec![rect_shape(1.0)]), 16);
        let has = |w: f64| rasters.get(&rect_shape(w).look_key().unwrap()).is_some();
        assert!(has(1.0) && !has(2.0) && has(3.0) && has(4.0));
    }

    #[test]
    fn saves_used_images_and_loads_them_back() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join(DIR);
        let rasters = Rasters::default();
        let (used, unused) = (rect_shape(40.0), rect_shape(60.0));
        rasters.insert(used.look_key().unwrap(), raster(200));
        rasters.insert(unused.look_key().unwrap(), raster(100));
        let set = annotations(vec![used.clone(), unused.clone()]);
        rasters.save(&folder, &set).unwrap();
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 3);

        let loaded = Rasters::load(&folder);
        assert_eq!(loaded.len(), 2);
        assert_eq!(*loaded.get(&used.look_key().unwrap()).unwrap(), raster(200));

        rasters
            .save(&folder, &annotations(vec![used.clone()]))
            .unwrap();
        let loaded = Rasters::load(&folder);
        assert_eq!(loaded.len(), 1);
        assert!(loaded.get(&unused.look_key().unwrap()).is_none());
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 2);

        assert!(Rasters::load(&dir.path().join("missing")).is_empty());
        let empty = dir.path().join("empty");
        Rasters::default()
            .save(&empty, &Annotations::default())
            .unwrap();
        assert!(!empty.exists());
    }

    #[test]
    fn cells_cover_the_area_in_texels() {
        let blur = |x: f64, width: f64, block: f64| AnnotationShape::Blur {
            rect: Rect {
                x,
                y: 10.0,
                width,
                height: 20.0,
            },
            block,
        };
        let grid = CellGrid::new(&blur(10.25, 30.0, 12.0), (100.0, 50.0), (200, 100)).unwrap();
        assert_eq!((grid.x, grid.y, grid.width, grid.height), (20, 20, 61, 40));
        assert_eq!((grid.block, grid.columns, grid.rows), (24, 3, 2));
        assert!(!grid.pixelate);

        let clipped = CellGrid::new(&blur(90.0, 30.0, 1.0), (100.0, 50.0), (200, 100)).unwrap();
        assert_eq!((clipped.x, clipped.width, clipped.block), (180, 20, 2));
        assert_eq!(
            CellGrid::new(&blur(120.0, 30.0, 12.0), (100.0, 50.0), (200, 100)),
            None
        );

        let huge = CellGrid::new(&blur(0.0, 3_000.0, 0.5), (3_000.0, 50.0), (6_000, 100)).unwrap();
        assert!(huge.columns <= MAX_CELLS && huge.block == 12);

        let line = AnnotationShape::Line {
            from: Point { x: 0.0, y: 0.0 },
            to: Point { x: 1.0, y: 1.0 },
            color: Color::rgb(0, 0, 0),
            width: 1.0,
        };
        assert_eq!(CellGrid::new(&line, (100.0, 50.0), (200, 100)), None);
    }

    #[test]
    fn packs_grids_in_rows() {
        let (corners, height) = pack(&[(3, 2), (4, 5), (2, 1), (6, 1)], 8);
        assert_eq!(corners, [(0, 0), (3, 0), (0, 5), (2, 5)]);
        assert_eq!(height, 6);
        assert_eq!(pack(&[], 8), (vec![], 0));
        assert_eq!(pack(&[(10, 2)], 8), (vec![(0, 0)], 2));
    }
}
