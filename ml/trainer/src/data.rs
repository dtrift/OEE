//! Dataset handling: CSV loading, the deterministic per-class split (a port of
//! `train_model_a.py`'s `split()`), the calibration sampler.
//!
//! Port facts (recorded in `fork/NOTES.md`):
//! - the split logic is identical (per-class shuffle, 85/15, the same
//!   `int(len * (1.0 - 0.15))` cut computed in f64) — the **class profiles**
//!   of both tracks' splits match exactly;
//! - the shuffle itself uses Rust's `StdRng` seeded with 2026, not numpy's
//!   PCG64: reproducing numpy's bit-exact shuffle is out of scope, so the
//!   window membership differs between the Python and Rust tracks while both
//!   stay deterministic run-to-run.

use std::path::Path;

use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};

use crate::{CHANNELS, NUM_CLASSES, SEED, TIMESTEPS};

/// One labeled window: `TIMESTEPS * CHANNELS` values, `(t, c)` row-major.
#[derive(Clone, Debug)]
pub struct Window {
    pub label: usize,
    pub values: Vec<f32>,
}

/// Loads `label,state,x000..x127` CSVs (the `line-simulator --dataset` format).
pub fn load_datasets(paths: &[std::path::PathBuf]) -> Result<Vec<Window>, String> {
    let mut windows = Vec::new();
    for path in paths {
        windows.extend(load_csv(path)?);
    }
    if windows.is_empty() {
        return Err("no windows loaded: pass at least one dataset CSV".into());
    }
    Ok(windows)
}

fn load_csv(path: &Path) -> Result<Vec<Window>, String> {
    let mut reader =
        csv::Reader::from_path(path).map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    let headers = reader
        .headers()
        .map_err(|e| format!("cannot read the header of {}: {e}", path.display()))?;
    let expected = 2 + TIMESTEPS * CHANNELS;
    if headers.len() != expected {
        return Err(format!(
            "{} has {} columns, the dataset format needs {expected} (label,state,x000..)",
            path.display(),
            headers.len()
        ));
    }
    let mut windows = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|e| format!("bad record in {}: {e}", path.display()))?;
        let label_field = record[0].to_string();
        let label: usize = label_field
            .parse()
            .map_err(|_| format!("bad label '{label_field}' in {}", path.display()))?;
        if label >= NUM_CLASSES {
            return Err(format!(
                "label {label} out of range in {} (0..{} expected)",
                path.display(),
                NUM_CLASSES - 1
            ));
        }
        let mut values = Vec::with_capacity(TIMESTEPS * CHANNELS);
        for field in record.iter().skip(2) {
            values.push(
                field
                    .parse()
                    .map_err(|_| format!("bad sample '{field}' in {}", path.display()))?,
            );
        }
        windows.push(Window { label, values });
    }
    Ok(windows)
}

/// The deterministic 85/15 split: per-class Fisher–Yates with the fixed seed
/// (both halves keep the class profile — the Python track's property).
pub fn split(windows: &[Window]) -> (Vec<Window>, Vec<Window>) {
    let mut rng = StdRng::seed_from_u64(SEED);
    let mut train = Vec::new();
    let mut val = Vec::new();
    for label in 0..NUM_CLASSES {
        let mut idx: Vec<usize> = windows
            .iter()
            .enumerate()
            .filter(|(_, w)| w.label == label)
            .map(|(i, _)| i)
            .collect();
        idx.shuffle(&mut rng);
        // The same cut as train_model_a.py: int(len * (1.0 - 0.15)) in f64,
        // at least one training window when the class is non-trivial.
        let cut = if idx.len() > 1 {
            ((idx.len() as f64) * (1.0 - 0.15)) as usize
        } else {
            idx.len()
        }
        .clamp(1, idx.len());
        for &i in &idx[..cut] {
            train.push(windows[i].clone());
        }
        for &i in &idx[cut..] {
            val.push(windows[i].clone());
        }
    }
    (train, val)
}

/// The representative-dataset sampler (the py script's
/// `representative_dataset`): `n` draws from the train set with the fixed
/// seed, with replacement.
pub fn calibration_windows(train: &[Window], n: usize) -> Vec<Vec<f32>> {
    let mut rng = StdRng::seed_from_u64(SEED);
    let mut picked = Vec::with_capacity(n);
    for _ in 0..n {
        let at = rng.random_range(0..train.len());
        picked.push(train[at].values.clone());
    }
    picked
}

/// Class weights: inverse frequency normalized to average 1 (the py script's
/// `class_weights`).
pub fn class_weights(labels: &[usize]) -> [f32; NUM_CLASSES] {
    let mut counts = [0usize; NUM_CLASSES];
    for &l in labels {
        counts[l] += 1;
    }
    let total = labels.len() as f32;
    let mut weights = [0.0f32; NUM_CLASSES];
    for (l, w) in weights.iter_mut().enumerate() {
        *w = total / (NUM_CLASSES as f32 * counts[l].max(1) as f32);
    }
    weights
}

/// Writes the val split as a `label,x000..` CSV (the metrics/parity input on
/// the Rust side; the py track's `model_a_val.npz` counterpart).
pub fn write_val_csv(val: &[Window], path: &Path) -> Result<(), String> {
    let mut out = String::from("label");
    for i in 0..TIMESTEPS * CHANNELS {
        out.push_str(&format!(",x{i:03}"));
    }
    out.push('\n');
    for window in val {
        out.push_str(&window.label.to_string());
        for v in &window.values {
            out.push_str(&format!(",{v}"));
        }
        out.push('\n');
    }
    std::fs::write(path, out).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic(n_per_class: usize) -> Vec<Window> {
        (0..NUM_CLASSES)
            .flat_map(|label| {
                (0..n_per_class).map(move |i| Window {
                    label,
                    values: vec![label as f32 + (i % 7) as f32 * 0.1; TIMESTEPS],
                })
            })
            .collect()
    }

    #[test]
    fn split_is_deterministic_and_keeps_profiles() {
        let windows = synthetic(100);
        let (train1, val1) = split(&windows);
        let (train2, val2) = split(&windows);
        assert_eq!(
            train1
                .iter()
                .map(|w| (w.label, w.values.clone()))
                .collect::<Vec<_>>(),
            train2
                .iter()
                .map(|w| (w.label, w.values.clone()))
                .collect::<Vec<_>>(),
            "the split must be deterministic"
        );
        assert_eq!(val1.len(), val2.len());
        // Class profiles: the cut matches the py formula exactly.
        for label in 0..NUM_CLASSES {
            let n = windows.iter().filter(|w| w.label == label).count();
            let expected_train = ((n as f64) * (1.0 - 0.15)) as usize;
            let got_train = train1.iter().filter(|w| w.label == label).count();
            let got_val = val1.iter().filter(|w| w.label == label).count();
            assert_eq!(got_train, expected_train, "class {label} train cut");
            assert_eq!(got_train + got_val, n, "class {label} total");
        }
    }

    #[test]
    fn class_weights_are_inverse_frequency() {
        // 8 windows: 4 of class 0, 2 of class 1, 1+1 of classes 2/3 — the py
        // formula total / (NUM_CLASSES * count).
        let labels = [0, 0, 0, 0, 1, 1, 2, 3];
        let w = class_weights(&labels);
        assert!((w[0] - 8.0 / (4.0 * 4.0)).abs() < 1e-6);
        assert!((w[1] - 8.0 / (4.0 * 2.0)).abs() < 1e-6);
        assert!((w[2] - 8.0 / (4.0 * 1.0)).abs() < 1e-6);
        assert!((w[3] - 8.0 / (4.0 * 1.0)).abs() < 1e-6);
    }

    #[test]
    fn calibration_sampler_is_deterministic() {
        let windows = synthetic(10);
        let a = calibration_windows(&windows, 8);
        let b = calibration_windows(&windows, 8);
        assert_eq!(a, b);
        assert_eq!(a.len(), 8);
    }
}
