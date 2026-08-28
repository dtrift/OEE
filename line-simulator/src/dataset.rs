//! Dataset windows for model A training (week 3, D4): the simulator stream
//! cut into `WindowSpec(A)` windows with ground-truth labels.
//!
//! Labeling rule: a window is kept only if the machine state is uniform
//! across all its samples — windows spanning a mode change are ambiguous and
//! dropped (the boundary contributes at most one window per transition).
//!
//! Export format (fixed, consumed by `ml/scripts/train_model_a.py`):
//! CSV, header `label,state,x000..xNNN`; `label` is the class index pinned by
//! [`crate::fsm::MachineState::class_index`]; `x###` are amperes, 6 decimals.

use crate::fsm::MachineState;
use crate::Sample;

/// One labeled training window.
#[derive(Debug, Clone, PartialEq)]
pub struct LabeledWindow {
    pub label: MachineState,
    pub samples: Vec<f32>,
}

/// Cuts the sample stream into labeled windows of `window_len` samples taken
/// every `stride` samples.
pub fn windows(samples: &[Sample], window_len: usize, stride: usize) -> Vec<LabeledWindow> {
    assert!(
        window_len > 0 && stride > 0,
        "window and stride must be positive"
    );
    let mut result = Vec::new();
    if samples.len() < window_len {
        return result;
    }
    let mut start = 0;
    while start + window_len <= samples.len() {
        let window = &samples[start..start + window_len];
        let label = window[0].state;
        if window.iter().all(|sample| sample.state == label) {
            result.push(LabeledWindow {
                label,
                samples: window.iter().map(|sample| sample.current_a).collect(),
            });
        }
        start += stride;
    }
    result
}

/// Writes the windows as a training CSV; returns the number of data rows.
pub fn write_csv(
    windows: &[LabeledWindow],
    mut writer: impl std::io::Write,
) -> std::io::Result<usize> {
    let window_len = windows.first().map(|w| w.samples.len()).unwrap_or(0);
    let mut header = vec!["label".to_string(), "state".to_string()];
    for i in 0..window_len {
        header.push(format!("x{i:03}"));
    }
    let mut text = String::new();
    text.push_str(&header.join(","));
    text.push('\n');
    for window in windows {
        let mut row = vec![
            window.label.class_index().to_string(),
            window.label.as_str().to_string(),
        ];
        row.extend(window.samples.iter().map(|s| format!("{s:.6}")));
        text.push_str(&row.join(","));
        text.push('\n');
    }
    writer.write_all(text.as_bytes())?;
    Ok(windows.len())
}

/// Per-class histogram (`[idle, run, jam, overload]` counts) — the D4 balance
/// check.
pub fn class_histogram(windows: &[LabeledWindow]) -> [usize; 4] {
    let mut histogram = [0usize; 4];
    for window in windows {
        histogram[window.label.class_index()] += 1;
    }
    histogram
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(t_ms: u32, current_a: f32, state: MachineState) -> Sample {
        Sample {
            t_ms,
            current_a,
            state,
        }
    }

    fn stream() -> Vec<Sample> {
        // 10 samples: 5 idle, 5 run — with a 4-window and stride 2:
        // starts 0 (idle-only), 2 (idle-only), 4 (spans), 6 (run-only)
        (0..5)
            .map(|i| sample(i, 0.1, MachineState::Idle))
            .chain((5..10).map(|i| sample(i, 1.0, MachineState::Run)))
            .collect()
    }

    #[test]
    fn uniform_windows_only() {
        // Windows start at 0, 2, 4, 6; the ones covering index 4->5 (the
        // idle/run boundary) span a state change and are dropped.
        let windows = windows(&stream(), 4, 2);
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].label, MachineState::Idle);
        assert_eq!(windows[1].label, MachineState::Run);
        assert_eq!(windows[0].samples, vec![0.1; 4]);
        assert_eq!(windows[1].samples, vec![1.0; 4]);
    }

    #[test]
    fn histogram_counts_classes() {
        let windows = windows(&stream(), 4, 2);
        assert_eq!(class_histogram(&windows), [1, 1, 0, 0]);
    }

    #[test]
    fn too_short_stream_yields_nothing() {
        assert!(windows(&stream()[..3], 4, 1).is_empty());
    }

    #[test]
    fn csv_has_pinned_header_and_rows() {
        // Stride 1, window 4: starts 0..=6; spans (2,3,4) are dropped,
        // keeping idle(0), idle(1), run(5), run(6).
        let windows = windows(&stream(), 4, 1);
        let mut buffer = Vec::new();
        let rows = write_csv(&windows, &mut buffer).unwrap();
        assert_eq!(rows, windows.len());
        assert_eq!(rows, 4);
        let text = String::from_utf8(buffer).unwrap();
        let mut lines = text.lines();
        assert_eq!(lines.next().unwrap(), "label,state,x000,x001,x002,x003");
        assert_eq!(
            lines.next().unwrap(),
            "0,idle,0.100000,0.100000,0.100000,0.100000"
        );
        assert_eq!(
            lines.next().unwrap(),
            "0,idle,0.100000,0.100000,0.100000,0.100000"
        );
        assert_eq!(
            lines.next().unwrap(),
            "1,run,1.000000,1.000000,1.000000,1.000000"
        );
        assert_eq!(
            lines.next().unwrap(),
            "1,run,1.000000,1.000000,1.000000,1.000000"
        );
    }
}
