//! Capture-CSV handling (decompose S3): the board lines become the host
//! run-CSV (`t_ms,current_a,state`) that `node --kind a` consumes.
//!
//! The capture schema (`features_cli::capture`):
//! `t_ms,node,run_id,value,state,note`; the converter filters `node=a`,
//! drops the run_id/note columns and passes `value` through as
//! `current_a` (the board already calibrated counts to amps). Malformed
//! rows are skipped and counted, never fatal — the same isolation rule as
//! the nodes.

/// The conversion outcome.
#[derive(Debug, Default, PartialEq)]
pub struct CaptureToRun {
    /// Rows written to the run CSV.
    pub rows: usize,
    /// Rows skipped (wrong node or malformed).
    pub skipped: usize,
}

/// Converts one capture-CSV line to a run-CSV line.
///
/// `None` — the line is not a node-a sample (header, other node, malformed).
pub fn capture_line_to_run(line: &str) -> Option<String> {
    let line = line.trim_end_matches(['\r', '\n']);
    if line.is_empty() {
        return None;
    }
    let mut fields = line.split(',');
    let t_ms = fields.next()?;
    let node = fields.next()?;
    if node != "a" {
        return None;
    }
    let _run_id = fields.next()?;
    let value = fields.next()?;
    let _state = fields.next().unwrap_or("");
    let _note = fields.next().unwrap_or("");
    // Both time and value must parse — and the value must be finite:
    // `"NaN".parse::<f32>()` succeeds in Rust, a NaN row must not reach
    // the model input.
    let t: u64 = t_ms.parse().ok()?;
    let v: f32 = value.parse().ok()?;
    if !v.is_finite() {
        return None;
    }
    Some(format!("{t},{v},"))
}

/// Converts a whole capture document (the host test path; the binary reads
/// stdin line by line with the same function).
pub fn capture_to_run(capture: &str) -> (String, CaptureToRun) {
    let mut out = String::from("t_ms,current_a,state\n");
    let mut stats = CaptureToRun::default();
    for line in capture.lines() {
        match capture_line_to_run(line) {
            Some(row) => {
                out.push_str(&row);
                out.push('\n');
                stats.rows += 1;
            }
            None => stats.skipped += 1,
        }
    }
    (out, stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_node_a_rows_and_drops_the_rest() {
        let capture = "\
t_ms,node,run_id,value,state,note
0,a,bench-a,0.012345,,
625,a,bench-a,0.023456,,
100,q,bench-q,0.5,,
garbage
";
        let (run, stats) = capture_to_run(capture);
        assert_eq!(run, "t_ms,current_a,state\n0,0.012345,\n625,0.023456,\n");
        assert_eq!(stats.rows, 2);
        // The header, the q row and the garbage line.
        assert_eq!(stats.skipped, 3);
    }

    #[test]
    fn a_bad_value_is_skipped_not_fatal() {
        let (run, stats) = capture_to_run("10,a,bench-a,NaN,,");
        assert_eq!(run, "t_ms,current_a,state\n");
        assert_eq!(stats.rows, 0);
        assert_eq!(stats.skipped, 1);
    }
}
