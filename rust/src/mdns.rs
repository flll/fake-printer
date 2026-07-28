//! mDNS / AirPrint advertisement. Port of `scripts/advertise-ipp-mdns.py`
//! (with the deployed hotfix: `_ipps` is intentionally NOT advertised — the
//! server speaks plain HTTP only; advertising IPPS makes clients attempt TLS
//! and fail ("offline")).

use std::net::{IpAddr, UdpSocket};

use mdns_sd::{ServiceDaemon, ServiceInfo};

use crate::config::Config;
use crate::events::{self, EventSender, UiEvent};

/// Best-effort primary IPv4 discovery (UDP connect trick, same as Python).
pub fn local_ipv4() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    socket.local_addr().ok().map(|a| a.ip())
}

fn txt_records(printer_name: &str) -> Vec<(&'static str, String)> {
    // AirPrint-compatible minimal TXT set — string-identical to the Python
    // advertiser (ty/note follow the configured printer name).
    vec![
        ("txtvers", "1".into()),
        ("qtotal", "1".into()),
        ("rp", "ipp/print".into()),
        ("ty", printer_name.into()),
        ("product", "(Virtual IPP PNG)".into()),
        ("note", "fake-printer — virtual IPP capture".into()),
        ("pdl", "application/pdf,image/pwg-raster,image/urf".into()),
        (
            "URF",
            "W8,SRGB24,CP255,DM1,FN3,IS0-0,MT1-8-11,OB10,PQ4-5,RS300,ST13,V1.4,W8".into(),
        ),
        ("Color", "T".into()),
        ("Duplex", "F".into()),
    ]
}

/// Register `_ipp._tcp` plus the AirPrint `_universal` subtype. Returns the
/// daemon which must be kept alive for the lifetime of the process.
pub fn advertise(config: &Config, events: &EventSender) -> Result<ServiceDaemon, mdns_sd::Error> {
    let daemon = ServiceDaemon::new()?;

    let ip = local_ipv4();
    let hostname = gethostname::gethostname().to_string_lossy().into_owned();
    let host_label = hostname.split('.').next().unwrap_or("fake-printer").to_string();
    let server_name = format!("{host_label}.local.");
    let instance = config.printer_name.replace(' ', "-");
    let props = txt_records(&config.printer_name);

    for service_type in ["_ipp._tcp.local.", "_universal._sub._ipp._tcp.local."] {
        let info = match ip {
            Some(ip) => ServiceInfo::new(
                service_type,
                &instance,
                &server_name,
                ip,
                config.listen_port,
                &props[..],
            )?,
            None => ServiceInfo::new(
                service_type,
                &instance,
                &server_name,
                (),
                config.listen_port,
                &props[..],
            )?
            .enable_addr_auto(),
        };
        daemon.register(info)?;
        tracing::info!(
            "registered {instance}.{service_type} on {}:{}",
            ip.map(|i| i.to_string()).unwrap_or_else(|| "<auto>".into()),
            config.listen_port,
        );
    }

    events::send(events, UiEvent::MdnsUp(true));
    Ok(daemon)
}
