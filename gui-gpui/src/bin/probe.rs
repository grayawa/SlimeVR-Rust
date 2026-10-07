use clap::Parser;
use slimevr_gpui::{client::Client, log_level::LogLevel};
use std::time::{Duration, Instant};

#[derive(Parser)]
#[command(about = "Read-only SolarXR diagnostic probe; does not change configuration")]
struct Options {
    #[arg(long, default_value = "ws://127.0.0.1:21110")]
    url: String,
    #[arg(long, default_value_t = 10, value_parser = clap::value_parser!(u64).range(1..=60))]
    wait_seconds: u64,
    #[arg(long, value_enum)]
    log_level: Option<LogLevel>,
}
fn main() -> Result<(), String> {
    let options = Options::parse();
    let client = Client::connect(options.url, LogLevel::resolve(options.log_level, false)?)?;
    let deadline = Instant::now() + Duration::from_secs(options.wait_seconds);
    loop {
        let snapshot = client.snapshot();
        if snapshot.feed.is_some()
            && snapshot.paused.is_some()
            && snapshot.settings.is_some()
            && snapshot.vrchat.is_some()
        {
            println!(
                "{}",
                serde_json::to_string_pretty(&snapshot).map_err(|e| e.to_string())?
            );
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(snapshot
                .last_error
                .unwrap_or_else(|| "Timed out waiting for backend state".into()));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
