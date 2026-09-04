//! The aggregator CLI (week 5, D2):
//!
//!     aggregator --mqtt 127.0.0.1:1883 --ideal-cycle-ms 400 \
//!         --out tmp/oee_windows.csv
//!
//! Subscribes to `oee/line1/{a/status, p/count, q/verdict, +/end}`, folds
//! minute windows + a cumulative shift view, publishes `oee/line1/oee` and
//! appends the windows CSV. Exits when every expected node (`--expect`,
//! default `a,p,q`) has published its stream-end marker — i.e. after the
//! bench run it accompanies. A broker that is down is a startup error (the
//! bench script starts the broker first); mid-run disconnects abort the
//! run — the CSV rows written so far stay usable.

use anyhow::Result;
use clap::Parser;

use oee_aggregator::aggregator::{self, Config};

/// The OEE aggregator: oee/line1/* -> windows -> oee/line1/oee + CSV.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// MQTT broker `host:port`.
    #[arg(long, default_value = "127.0.0.1:1883")]
    mqtt: String,
    /// Topic prefix of the line.
    #[arg(long, default_value = "oee/line1")]
    prefix: String,
    /// Nominal ideal cycle time of the line, ms (a line property: slowdown
    /// scenarios keep the nominal ideal).
    #[arg(long, default_value_t = 400)]
    ideal_cycle_ms: u32,
    /// The minute-window length, ms of machine time.
    #[arg(long, default_value_t = 60_000)]
    minute_ms: u32,
    /// Node streams to expect (the final flush waits for their end
    /// markers), comma-separated (a subset of a, p, q).
    #[arg(long, default_value = "a,p,q")]
    expect: String,
    /// Where to write the windows CSV (scope,run_id,t_from_ms,…).
    #[arg(long, default_value = "tmp/oee_windows.csv")]
    out: std::path::PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let expect_nodes: Vec<String> = args
        .expect
        .split(',')
        .map(|node| node.trim().to_ascii_lowercase())
        .filter(|node| matches!(node.as_str(), "a" | "p" | "q"))
        .collect();
    if expect_nodes.is_empty() {
        anyhow::bail!(
            "--expect must list at least one of a, p, q (got {:?})",
            args.expect
        );
    }
    let config = Config {
        broker_addr: args.mqtt.clone(),
        topic_prefix: args.prefix.clone(),
        ideal_cycle_ms: args.ideal_cycle_ms,
        minute_ms: args.minute_ms,
        expect_nodes,
        csv_path: Some(args.out.clone()),
        ready: None,
    };
    eprintln!(
        "aggregator: {} windows of {} ms, ideal cycle {} ms, out {}",
        config.expect_nodes.join("+"),
        config.minute_ms,
        config.ideal_cycle_ms,
        args.out.display()
    );
    let summary = aggregator::run(&config)
        .map_err(|error| anyhow::anyhow!("aggregating from {}: {error}", args.mqtt))?;
    println!(
        "aggregator: {} messages, {} parse errors, {} minute windows, {} publishes",
        summary.messages, summary.parse_errors, summary.windows, summary.publishes
    );
    if let Some(shift) = summary.final_shift {
        println!(
            "aggregator: OEE {:.3} (A {:.3}, P {:.3}, Q {:.3}) over {} ms, {} parts",
            shift.oee,
            shift.availability,
            shift.performance,
            shift.quality,
            shift.t_to_ms,
            shift.parts
        );
    }
    Ok(())
}
