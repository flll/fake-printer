//! Events streamed from the server to the dashboard. Replaces the Python
//! approach of tailing server.log with regexes.

#[derive(Debug, Clone)]
pub enum UiEvent {
    /// A document-carrying job started (Print-Job or Send-Document).
    JobStarted { name: String },
    /// Pages written to the spool so far for the current job.
    JobProgress { pages_seen: usize },
    /// Render finished; job appears in "Recent activity" with mode pending.
    JobDone { token: String, name: String, pages: usize },
    /// Postprocess finished; updates the recent entry's mode badge.
    JobMode { token: String, mode: String },
    /// Render failed; clears the "now rendering" line.
    JobFailed,
    /// mDNS registration status.
    MdnsUp(bool),
}

pub type EventSender = tokio::sync::mpsc::UnboundedSender<UiEvent>;

pub fn send(events: &EventSender, event: UiEvent) {
    let _ = events.send(event);
}
