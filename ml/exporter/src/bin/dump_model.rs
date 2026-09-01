//! Dumps the structure of a `.tflite` (track D1): the rust-born analog of the
//! week-1 `conv1d_ops.txt`, for diffing against the TF-converted file.
//!
//!     cargo run -p exporter --bin dump_model -- ml/models/model_a.tflite

use std::path::PathBuf;

fn main() {
    let path: PathBuf = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("ml/models/conv1d.tflite"));
    match exporter::dumper::dump_file(&path) {
        Ok(dump) => print!("{dump}"),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
