//! The S3 cross-check utility: board capture CSV → host run CSV.
//!
//!     cat bench-a.capture.csv | cargo run -p firmware-tools --bin capture-to-run > run.csv
//!     cargo run --release -p nodes --bin node -- --kind a --input run.csv \
//!         --offline statuses.csv --mqtt 127.0.0.1:1883 --run-id bench-a
//!
//! The same physical run then flows through the host node A; a status
//! mismatch table (board vs host) lands in firmware/NOTES.md (decompose S3).

use std::io::{Read, Write};

fn main() {
    let mut capture = String::new();
    std::io::stdin()
        .read_to_string(&mut capture)
        .expect("read stdin");
    let (run, stats) = firmware_tools::capture::capture_to_run(&capture);
    std::io::stdout()
        .write_all(run.as_bytes())
        .expect("write stdout");
    eprintln!(
        "capture-to-run: {} rows, {} skipped (header/other-node/bad)",
        stats.rows, stats.skipped
    );
}
