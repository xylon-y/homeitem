//! Ensure the frontend dist directory exists so rust-embed can find it.
//! trunk build fills it; a bare cargo build still succeeds with a stub.

use std::fs;
use std::path::Path;

fn main() {
    let dist = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/frontend/dist");
    println!("cargo:rerun-if-changed=../../crates/frontend/dist");
    if !dist.exists() {
        let _ = fs::create_dir_all(&dist);
        let placeholder = dist.join("index.html");
        if !placeholder.exists() {
            let _ = fs::write(
                &placeholder,
                "<!doctype html><html><head><meta charset=\"utf-8\"><title>homeitem</title></head><body>frontend not built, run trunk build</body></html>",
            );
        }
    }
}
