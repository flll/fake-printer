//! fake-printer: virtual IPP/AirPrint printer that captures LAN print jobs
//! into an agent-friendly inbox. Single-binary Rust port of the Python
//! implementation (paperlessprinter engine + wrapper scripts).
//!
//! License: AGPL-3.0 (port of the AGPL paperlessprinter engine).

mod config;
mod events;
mod ipp;
mod jobs;
mod mdns;
mod postprocess;
mod render;
mod server;
mod single_instance;
mod tui;
mod upload;

use std::sync::Arc;

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::config::Config;
use crate::events::UiEvent;
use crate::jobs::JobRegistry;
use crate::server::AppState;

fn init_logging(config: &Config, headless: bool) -> tracing_appender::non_blocking::WorkerGuard {
    let level = config.log_level.to_lowercase();
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(format!("fake_printer={level}")));

    let _ = std::fs::create_dir_all(&config.logs_dir);
    let file_appender = tracing_appender::rolling::never(&config.logs_dir, "server.log");
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(file_writer)
        .with_ansi(false);

    if headless {
        // Also log to stderr when running without the TUI.
        let stderr_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stderr);
        tracing_subscriber::registry()
            .with(filter)
            .with(file_layer)
            .with(stderr_layer)
            .init();
    } else {
        tracing_subscriber::registry().with(filter).with(file_layer).init();
    }
    guard
}

/// Another instance owns the port. Keep it running and give the user a few
/// seconds to read why this window is closing (the launcher .bat exits with us).
fn report_already_running(port: u16) {
    use std::io::Write;

    let msg = format!("fake-printer is already running on port {port} — leaving it untouched.");
    tracing::warn!("{msg}");
    println!("{msg}");
    for remaining in (1..=5).rev() {
        print!("\rClosing this window in {remaining}s... ");
        let _ = std::io::stdout().flush();
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    println!();
}

fn main() -> std::io::Result<()> {
    let headless = std::env::args().any(|a| a == "--headless");
    let config = Config::load();
    let _log_guard = init_logging(&config, headless);
    if let Some(warning) = &config.env_warning {
        eprintln!("[WARN] {warning}");
        tracing::warn!("{warning}");
    }

    let Some(_instance_lock) = single_instance::acquire(config.listen_port) else {
        report_already_running(config.listen_port);
        return Ok(());
    };

    // The exe is launched directly (no wrapper .bat), so name the window here.
    let _ = ratatui::crossterm::execute!(
        std::io::stdout(),
        ratatui::crossterm::terminal::SetTitle(format!("fake-printer - {}", config.printer_name))
    );

    // Fail fast if the embedded pdfium.dll cannot be extracted.
    match render::ensure_pdfium_dll() {
        Ok(path) => tracing::info!("pdfium ready: {}", path.display()),
        Err(e) => {
            eprintln!("[ERROR] pdfium.dll extract failed: {e}");
            return Err(e);
        }
    }

    let runtime = tokio::runtime::Runtime::new()?;
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<UiEvent>();

    // Bind before advertising: publishing mDNS from a process that then fails to
    // listen leaves clients with a dead service record for this printer name.
    let listener = match runtime.block_on(server::bind(&config)) {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!(
                "[ERROR] cannot listen on {}:{}: {e}",
                config.listen_host, config.listen_port
            );
            tracing::error!("bind failed: {e}");
            return Err(e);
        }
    };

    let state = Arc::new(AppState {
        config: config.clone(),
        registry: JobRegistry::new(),
        events: tx.clone(),
    });

    // IPP server.
    let server_state = Arc::clone(&state);
    runtime.spawn(async move {
        if let Err(e) = server::serve(listener, server_state).await {
            tracing::error!("server exited: {e}");
        }
    });

    // mDNS advertisement (daemon runs its own thread; keep handle alive).
    let mdns_daemon = match mdns::advertise(&config, &tx) {
        Ok(daemon) => Some(daemon),
        Err(e) => {
            tracing::error!("mDNS advertise failed: {e}");
            events::send(&tx, UiEvent::MdnsUp(false));
            None
        }
    };

    let ip = mdns::local_ipv4()
        .map(|i| i.to_string())
        .unwrap_or_else(|| "127.0.0.1".into());
    let ipp_url = format!("ipp://{}:{}{}", ip, config.listen_port, config.ipp_path);
    println!(
        "Listening on http://{}:{}{}",
        config.listen_host, config.listen_port, config.ipp_path
    );

    let exit = if headless {
        runtime.block_on(async {
            let _ = tokio::signal::ctrl_c().await;
        });
        Ok(())
    } else {
        match tui::run(
            tui::TuiContext {
                printer_name: config.printer_name.clone(),
                config,
                ipp_url,
            },
            rx,
        ) {
            Ok(()) => Ok(()),
            Err(e) => {
                tracing::warn!("TUI unavailable ({e}); running headless — Ctrl+C to stop");
                runtime.block_on(async {
                    let _ = tokio::signal::ctrl_c().await;
                });
                Ok(())
            }
        }
    };

    if let Some(daemon) = mdns_daemon {
        let _ = daemon.shutdown();
    }
    runtime.shutdown_timeout(std::time::Duration::from_secs(2));
    exit
}
