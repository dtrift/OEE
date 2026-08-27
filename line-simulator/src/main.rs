//! Simulator CLI: `line-simulator --scenario base.toml --seed 42 --out run1.csv`.

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;

use line_simulator::scenario::{Scenario, SAMPLE_RATE_HZ};
use line_simulator::Simulator;

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
    /// Where to write the CSV (t_ms,current_a,state).
    #[arg(long)]
    out: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let text = fs::read_to_string(&args.scenario)
        .with_context(|| format!("reading {}", args.scenario.display()))?;
    let scenario = Scenario::parse(&text).map_err(anyhow::Error::msg)?;

    let stdout = std::io::stdout();
    let out: Box<dyn Write> = if args.out.as_os_str() == "-" {
        Box::new(stdout.lock())
    } else {
        Box::new(
            fs::File::create(&args.out)
                .with_context(|| format!("creating {}", args.out.display()))?,
        )
    };

    let mut writer = csv::Writer::from_writer(out);
    writer.write_record(["t_ms", "current_a", "state"])?;

    let mut simulator = Simulator::new(args.seed, scenario.signal);
    let total_samples = (scenario.duration_ms as u64 * SAMPLE_RATE_HZ as u64) / 1000;
    let mut next_event = 0usize;

    for _ in 0..total_samples {
        while next_event < scenario.events.len()
            && scenario.events[next_event].t_ms <= simulator.next_t_ms()
        {
            simulator.apply(scenario.events[next_event].state);
            next_event += 1;
        }
        let sample = simulator.next_sample(&scenario.envelope, &scenario.noise);
        writer.write_record([
            sample.t_ms.to_string(),
            format!("{:.4}", sample.current_a),
            sample.state.as_str().to_string(),
        ])?;
    }
    writer.flush()?;
    eprintln!(
        "written {} samples ({} ms) to {}",
        total_samples,
        scenario.duration_ms,
        args.out.display()
    );
    Ok(())
}
