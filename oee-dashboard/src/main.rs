//! The dashboard binary (week 5, D3):
//!
//!     cargo run -p oee-dashboard -- --mqtt 127.0.0.1:1883
//!
//! Subscribes to `oee/line1/#` over mqtt-min and renders the live OEE view:
//! the shift OEE/A/P/Q gauges (zones: green >= 85%, yellow >= 60%), the
//! part counter, the machine status, the Q verdict ticker, and the
//! minute-window OEE sparkline. `q` exits; a dropped broker shows as a
//! red "reconnecting" status and heals itself.

mod mqtt;
mod state;
mod ui;

use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::Result;
use clap::Parser;

use mqtt::MqttEvent;
use state::DashboardState;

/// The redraw cadence, ~5 fps.
const TICK: Duration = Duration::from_millis(200);

/// The ratatui TUI dashboard of the OEE bench.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// MQTT broker `host:port`.
    #[arg(long, default_value = "127.0.0.1:1883")]
    mqtt: String,
    /// Topic filter of the line.
    #[arg(long, default_value = "oee/line1/#")]
    filter: String,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // ratatui::init sets a panic hook that restores the terminal — a panic
    // in the render leaves the shell usable.
    let mut terminal = ratatui::init();
    let (events_tx, events_rx) = mpsc::channel();
    // The thread ends by itself once `events_tx` is dropped here — no join
    // needed (the handle is kept for the intent, not used).
    let _mqtt_thread = std::thread::spawn({
        let addr = args.mqtt.clone();
        let filter = args.filter.clone();
        move || mqtt::run_loop(addr, filter, events_tx)
    });

    let mut state = DashboardState::new(&args.mqtt);
    let exit = || -> Result<()> {
        ratatui::restore();
        Ok(())
    };
    loop {
        // Drain the MQTT events (non-blocking; the channel closes when the
        // thread is gone — the thread only exits when we do).
        while let Ok(event) = events_rx.try_recv() {
            match event {
                MqttEvent::Message { topic, payload } => {
                    state.on_message(&topic, &payload, Instant::now())
                }
                MqttEvent::Connected => state.connected = true,
                MqttEvent::Disconnected => state.connected = false,
                MqttEvent::Tick => {}
            }
        }
        if ratatui::crossterm::event::poll(TICK)? {
            if let ratatui::crossterm::event::Event::Key(key) = ratatui::crossterm::event::read()? {
                if key.code == ratatui::crossterm::event::KeyCode::Char('q') {
                    return exit();
                }
            }
        }
        terminal.draw(|frame| ui::ui(frame, &state, Instant::now()))?;
    }
}
