//! Host-side bench tools (the hardware track, decompose S3/S7 and phase 2).
//!
//! Two binaries live here:
//! - `capture-to-run` — the board capture CSV becomes the host node input
//!   (the S3 cross-check: the same physical run through the board and the
//!   host node A);
//! - `uart-bridge` — reads the A/P/Q status lines from stdin and publishes
//!   them to MQTT on `oee/line1/*` (phase 2: the bench joins the week-5
//!   full loop without a line of network code on the boards).
//!
//! The library part is the shared line-parsing logic (host-tested).

pub mod bridge;
pub mod capture;
