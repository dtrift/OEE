//! The node CLI (week 4, D1/D2/D4; node P — week 5, D1):
//!
//!     node --kind a --input tmp/run1.csv --offline tmp/statuses.csv
//!     node --kind q --input tmp/taps.csv --meta tmp/taps_meta.csv \
//!         --offline tmp/verdicts.csv --mqtt 127.0.0.1:1883
//!     node --kind p --input tmp/ir.csv --offline tmp/counts.csv \
//!         --mqtt 127.0.0.1:1883
//!
//! Offline mode is the base (the D1 artifact); MQTT is layered on top and
//! degrades to offline-only when the broker is unreachable (D5). The meta
//! topic is published once at startup when MQTT is on; the `{node}/end`
//! marker (the aggregator's flush signal) is published once at stream end.

use std::fs::File;

use anyhow::{Context, Result};
use clap::Parser;
use nodes::mqtt_sink::MqttSink;
use nodes::sim_source::{SimSource, TapSource};
use nodes::status::{CsvStatusLog, MultiSink};

/// Which node to run.
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum Kind {
    /// Node A: current -> status (input: run CSV).
    A,
    /// Node P: IR-barrier events -> part count (input: belt-events CSV).
    P,
    /// Node Q: taps -> verdict (input: taps dataset + meta CSVs).
    Q,
}

impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Kind::A => "a",
            Kind::P => "p",
            Kind::Q => "q",
        })
    }
}

/// A digital-twin node: source -> windows -> predict -> status sink.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Which node to run.
    #[arg(long)]
    kind: Kind,
    /// Input CSV: the simulator run CSV (`--out`) for A, the taps dataset
    /// for Q, the belt-events CSV for P.
    #[arg(long)]
    input: std::path::PathBuf,
    /// The taps meta CSV (`t_ms,verdict`), node Q only (timestamps).
    #[arg(long)]
    meta: Option<std::path::PathBuf>,
    /// Where to write the offline status CSV (node,run_id,t_ms,state).
    #[arg(long)]
    offline: std::path::PathBuf,
    /// Run identifier (status rows, MQTT payloads).
    #[arg(long, default_value = "run1")]
    run_id: String,
    /// MQTT broker `host:port` (optional; offline CSV is written regardless).
    #[arg(long)]
    mqtt: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    match args.kind {
        Kind::A => {
            let input = File::open(&args.input)
                .with_context(|| format!("opening {}", args.input.display()))?;
            let mut source = SimSource::new(input);
            let mut log = CsvStatusLog::new(
                File::create(&args.offline)
                    .with_context(|| format!("creating {}", args.offline.display()))?,
            )?;
            let summary = match &args.mqtt {
                Some(addr) => {
                    let mut mqtt = MqttSink::new(addr, &format!("node-{}", args.kind));
                    mqtt.publish_a_meta("model_a.tflite", nodes::a::WINDOW, 1600);
                    let mut sink = MultiSink(&mut log, &mut mqtt);
                    let summary = nodes::a::run_a(&mut source, &args.run_id, &mut sink);
                    // The stream-end marker carries the stream's last time —
                    // the aggregator closes its final window at the max of
                    // these (for node A it is the scenario end minus a step).
                    mqtt.publish_end("a", source.last_t_ms(), &args.run_id);
                    println!(
                        "mqtt: {} published, {} failed (offline CSV intact)",
                        mqtt.publishes(),
                        mqtt.failures()
                    );
                    summary
                }
                None => nodes::a::run_a(&mut source, &args.run_id, &mut log),
            };
            log.flush()?;
            report(
                args.kind,
                summary.windows,
                summary.dirty_windows,
                summary.statuses,
            );
        }
        Kind::P => {
            let input = File::open(&args.input)
                .with_context(|| format!("opening {}", args.input.display()))?;
            let mut source = nodes::sim_source::IrSource::new(input);
            let mut log = CsvStatusLog::new(
                File::create(&args.offline)
                    .with_context(|| format!("creating {}", args.offline.display()))?,
            )?;
            let summary = match &args.mqtt {
                Some(addr) => {
                    let mut mqtt = MqttSink::new(addr, &format!("node-{}", args.kind));
                    mqtt.publish_p_meta();
                    let mut sink = MultiSink(&mut log, &mut mqtt);
                    let summary = nodes::p::run_p(&mut source, &args.run_id, &mut sink);
                    mqtt.publish_end("p", source.last_t_ms(), &args.run_id);
                    println!(
                        "mqtt: {} published, {} failed (offline CSV intact)",
                        mqtt.publishes(),
                        mqtt.failures()
                    );
                    summary
                }
                None => nodes::p::run_p(&mut source, &args.run_id, &mut log),
            };
            log.flush()?;
            println!(
                "node p: {} rising edges, {} merged (doubles), {} parts, {} bad rows -> offline CSV",
                summary.rising_edges, summary.merged, summary.parts, summary.bad_rows,
            );
        }
        Kind::Q => {
            let meta_path = args
                .meta
                .as_ref()
                .context("node Q needs --meta (the taps meta CSV for timestamps)")?;
            let input = File::open(&args.input)
                .with_context(|| format!("opening {}", args.input.display()))?;
            let meta = File::open(meta_path)
                .with_context(|| format!("opening {}", meta_path.display()))?;
            let mut source = TapSource::new(input, meta);
            let mut log = CsvStatusLog::new(
                File::create(&args.offline)
                    .with_context(|| format!("creating {}", args.offline.display()))?,
            )?;
            let summary = match &args.mqtt {
                Some(addr) => {
                    let mut mqtt = MqttSink::new(addr, &format!("node-{}", args.kind));
                    mqtt.publish_q_meta("model_q.tflite", nodes::q::WINDOW, 16_000);
                    let mut sink = MultiSink(&mut log, &mut mqtt);
                    let summary = nodes::q::run_q(&mut source, &args.run_id, &mut sink);
                    mqtt.publish_end("q", source.last_t_ms(), &args.run_id);
                    println!(
                        "mqtt: {} published, {} failed (offline CSV intact)",
                        mqtt.publishes(),
                        mqtt.failures()
                    );
                    summary
                }
                None => nodes::q::run_q(&mut source, &args.run_id, &mut log),
            };
            log.flush()?;
            report(
                args.kind,
                summary.windows,
                summary.dirty_windows,
                summary.verdicts,
            );
        }
    }
    Ok(())
}

fn report(kind: Kind, windows: usize, dirty: usize, emitted: usize) {
    println!(
        "node {kind}: {windows} windows, {dirty} dropped (bad rows), {emitted} status rows -> offline CSV",
    );
}
