/// Simulator CLI:
/// `line-simulator --scenario base.toml --seed 42 --out run1.csv`
/// (raw stream), or `line-simulator --scenario base.toml --seed 42
/// --dataset windows.csv [--stride 64]` (labeled training windows, D4).
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;

use line_simulator::scenario::{Scenario, SAMPLE_RATE_HZ};
use line_simulator::{dataset, Simulator};

/// Deterministic production-line simulator.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Path to the TOML scenario (run ground truth).
    #[arg(long)]
    scenario: PathBuf,
    /// Noise generator seed (determinism: one seed -> one CSV).
    #[arg(long, default_value_t = 42)]
    seed: u64,
    /// Where to write the raw CSV (t_ms,current_a,state).
    #[arg(long, requires = "scenario")]
    out: Option<PathBuf>,
    /// Where to write the labeled training windows (label,state,x000..)
    /// instead of the raw stream.
    #[arg(long, conflicts_with = "out")]
    dataset: Option<PathBuf>,
    /// Window stride for --dataset, samples (default: half a window).
    #[arg(long, default_value_t = 64)]
    stride: usize,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let text = fs::read_to_string(&args.scenario)
        .with_context(|| format!("reading {}", args.scenario.display()))?;
    let scenario = Scenario::parse(&text).map_err(anyhow::Error::msg)?;

    let mut simulator = Simulator::new(args.seed, scenario.signal);
    let total_samples = (scenario.duration_ms as u64 * SAMPLE_RATE_HZ as u64) / 1000;
    let mut next_event = 0usize;
    let mut samples = Vec::with_capacity(total_samples as usize);

    for _ in 0..total_samples {
        while next_event < scenario.events.len()
            && scenario.events[next_event].t_ms <= simulator.next_t_ms()
        {
            simulator.apply(scenario.events[next_event].state);
            next_event += 1;
        }
        samples.push(simulator.next_sample(&scenario.envelope, &scenario.noise));
    }

    match (args.out, args.dataset) {
        (Some(out), _) => write_raw(out, &samples),
        (None, Some(out)) => write_dataset(out, &samples, args.stride),
        (None, None) => {
            anyhow::bail!("nothing to do: pass --out or --dataset")
        }
    }
}

fn write_raw(out: PathBuf, samples: &[line_simulator::Sample]) -> Result<()> {
    let stdout = std::io::stdout();
    let sink: Box<dyn Write> = if out.as_os_str() == "-" {
        Box::new(stdout.lock())
    } else {
        Box::new(fs::File::create(&out).with_context(|| format!("creating {}", out.display()))?)
    };
    let mut writer = csv::Writer::from_writer(sink);
    writer.write_record(["t_ms", "current_a", "state"])?;
    for sample in samples {
        writer.write_record([
            sample.t_ms.to_string(),
            format!("{:.4}", sample.current_a),
            sample.state.as_str().to_string(),
        ])?;
    }
    writer.flush()?;
    eprintln!("written {} samples to {}", samples.len(), out.display());
    Ok(())
}

fn write_dataset(out: PathBuf, samples: &[line_simulator::Sample], stride: usize) -> Result<()> {
    // The window length is the node A model contract (WindowSpec(A) = 128).
    let window_len = 128;
    let windows = dataset::windows(samples, window_len, stride);
    let histogram = dataset::class_histogram(&windows);
    let file = fs::File::create(&out).with_context(|| format!("creating {}", out.display()))?;
    let rows = dataset::write_csv(&windows, file)?;
    eprintln!(
        "written {rows} windows ({window_len} samples, stride {stride}) to {}",
        out.display()
    );
    eprintln!("classes [idle, run, jam, overload]: {histogram:?}");
    Ok(())
}
