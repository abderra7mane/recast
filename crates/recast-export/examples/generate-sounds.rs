//! Writes the bundled click sounds to `assets/sounds`.

use std::path::PathBuf;

use recast_export::sounds::{SoundKind, synthesize, wav};
use recast_project::SoundPack;

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/sounds");
    for pack in SoundPack::ALL {
        let dir = root.join(pack.id());
        std::fs::create_dir_all(&dir).expect("create sound folder");
        for kind in SoundKind::ALL {
            let path = dir.join(kind.file_name());
            std::fs::write(&path, wav(&synthesize(pack, kind))).expect("write sound");
            println!("wrote {}", path.display());
        }
    }
}
