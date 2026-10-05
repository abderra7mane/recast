use std::path::PathBuf;

fn main() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(recast_desktop_lib::BINDINGS_PATH);
    if let Err(e) = recast_desktop_lib::export_bindings(&path) {
        eprintln!("cannot export bindings: {e}");
        std::process::exit(1);
    }
    println!("wrote {}", path.display());
}
