//! Serves romforge's games to uQ's launcher on the session bus and exits
//! after 60 s without a call, never while a game it started is running.
//! The bus starts it again (`org.romforge.Provider.service`) when uQ's
//! launcher opens, so it only runs while it is useful.

use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use romforge_provider::{BUS_NAME, Games, PATH, Provider};
use tokio::sync::Notify;

const IDLE: Duration = Duration::from_secs(60);
/// Testing aid: a shorter idle time, 1-3600 seconds.
const IDLE_ENV: &str = "ROMFORGE_PROVIDER_IDLE_SECS";

fn idle() -> Duration {
    std::env::var(IDLE_ENV)
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|s| (1..=3600).contains(s))
        .map(Duration::from_secs)
        .unwrap_or(IDLE)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    if std::env::args().nth(1).is_some_and(|a| a == "--version") {
        println!("romforge-provider {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    let games = Arc::new(Games::new(romforge_core::db_path()));
    let touched = Arc::new(Notify::new());
    let provider = Provider {
        games: games.clone(),
        touched: touched.clone(),
    };
    let built = zbus::connection::Builder::session()
        .name(BUS_NAME)
        .serve_at(PATH, provider)
        .build()
        .await;
    let _conn = match built {
        Ok(c) => c,
        Err(e) => {
            eprintln!("romforge-provider: cannot own {BUS_NAME} on the session bus: {e}");
            return ExitCode::FAILURE;
        }
    };
    // The list is built while uQ's launcher is still opening.
    let warming = games.clone();
    tokio::task::spawn_blocking(move || {
        if let Err(e) = warming.warm() {
            eprintln!("romforge-provider: {e}");
        }
    });
    // Exit once a whole idle period passes without a call and with no
    // game of ours running; the bus starts us again when needed.
    let idle = idle();
    loop {
        if tokio::time::timeout(idle, touched.notified()).await.is_err() && !games.playing() {
            break;
        }
    }
    ExitCode::SUCCESS
}
