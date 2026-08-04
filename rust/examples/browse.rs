//! Dev tool: browse `_ipp._tcp.local.` for a few seconds and print what the
//! LAN (including this host's fake-printer) is advertising.
//!
//! Usage: cargo run --example browse

use mdns_sd::{ServiceDaemon, ServiceEvent};

fn main() {
    let daemon = ServiceDaemon::new().expect("daemon");
    let receiver = daemon.browse("_ipp._tcp.local.").expect("browse");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(4);

    while let Ok(event) = receiver.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now())) {
        if let ServiceEvent::ServiceResolved(info) = event {
            println!(
                "resolved: {} -> {:?}:{} txt.ty={:?}",
                info.get_fullname(),
                info.get_addresses(),
                info.get_port(),
                info.get_property_val_str("ty"),
            );
        }
        if std::time::Instant::now() >= deadline {
            break;
        }
    }
    let _ = daemon.shutdown();
}
