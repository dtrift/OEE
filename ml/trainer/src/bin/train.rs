//! Track D5: the one-command pipeline.
//!
//!     cargo run -p trainer --release --bin train -- \
//!         --datasets tmp/ds_base_1.csv tmp/ds_base_2.csv ... \
//!         --calib 256 --out ml/models/model_a.tflite
//!
//! Week 4 (D4): `--task q` trains node Q's model on tap datasets
//! (`line-simulator --taps-dataset`), defaulting the output to
//! `ml/models/model_q.tflite`.
//!
//! Artifacts (next to the .tflite): `<model>.float` (float weights),
//! `<model>_val.csv` (the val split, the metrics/parity input), the ops dump.
//! After this, re-run the microflow-side checks (they bake the file at
//! compile time — see ml/README.md):
//!     touch ml/exporter/tests/model_a_parity.rs nodes/src/a.rs
//!     cargo test -p exporter --test model_a_parity --release
//!     cargo test -p exporter --test ml_metrics --release -- --nocapture

use std::path::PathBuf;

use clap::Parser;

/// Which node's model to train.
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum Task {
    /// Node A: machine current, 128 @ 1.6 kHz, 4 classes.
    A,
    /// Node Q: tap audio, 1024 @ 16 kHz, 2 classes.
    Q,
}

impl std::fmt::Display for Task {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.spec().name())
    }
}

impl Task {
    fn spec(self) -> trainer::TaskSpec {
        match self {
            Task::A => trainer::TaskSpec::a(),
            Task::Q => trainer::TaskSpec::q(),
        }
    }

    fn default_out(self) -> &'static str {
        match self {
            Task::A => "ml/models/model_a.tflite",
            Task::Q => "ml/models/model_q.tflite",
        }
    }
}

/// Trains a node model in burn and exports the int8 `.tflite` (Rust-only
/// pipeline).
#[derive(Parser, Debug)]
#[command(about, long_about = None)]
struct Args {
    /// Dataset CSVs from `line-simulator --dataset` (A) or `--taps-dataset`
    /// (Q): repeat the flag or pass several values: --datasets a.csv b.csv.
    #[arg(long, required = true, num_args = 1..)]
    datasets: Vec<PathBuf>,
    /// Which node's model to train.
    #[arg(long, default_value_t = Task::A)]
    task: Task,
    /// Calibration windows for PTQ (drawn from the train split, seed 2026).
    #[arg(long, default_value_t = 256)]
    calib: usize,
    /// Training epochs.
    #[arg(long, default_value_t = 30)]
    epochs: usize,
    /// The int8 model path (defaults per task).
    #[arg(long)]
    out: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let spec = args.task.spec();
    let out = args.out.unwrap_or_else(|| args.task.default_out().into());
    let float_out = out.with_extension("float");
    let val_csv = out.with_extension("val.csv");
    let report = trainer::pipeline::run::<burn::backend::Autodiff<burn::backend::ndarray::NdArray>>(
        &spec,
        trainer::pipeline::PipelineArgs {
            datasets: args.datasets,
            epochs: args.epochs,
            calib: args.calib,
            out: out.clone(),
            float_out,
            val_csv,
        },
    )?;

    // The metrics file in the py script's format (int8 through microflow is a
    // separate test target — ml_metrics — since #[model] is compile-time).
    let mut metrics = vec![
        format!("task: {}", spec.name()),
        format!("seed: {}", trainer::SEED),
        format!("float val accuracy: {:.4}", report.float_val_accuracy),
        format!(
            "int8 val accuracy (interp): {:.4}",
            report.int8_val_accuracy
        ),
        format!("windows: val {}", report.val_windows),
        format!("sha256: {}", report.sha256),
        "confusion matrix (rows=true, cols=pred):".to_string(),
    ];
    let header: String = spec
        .class_names
        .iter()
        .map(|n| format!("{n:>10}"))
        .collect();
    metrics.push(format!("       {header}"));
    for (label, row) in spec.class_names.iter().zip(&report.confusion) {
        let cells: String = row.iter().map(|v| format!("{v:>10}")).collect();
        metrics.push(format!("{label:>6} {cells}"));
    }
    let metrics_path = out.with_extension("metrics.txt");
    std::fs::write(&metrics_path, metrics.join("\n") + "\n")?;
    println!("metrics: {}", metrics_path.display());
    Ok(())
}
