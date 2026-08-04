//! Foreground console dashboard. Port of the rich UI in
//! `scripts/dashboard.py`, driven by direct events instead of log tailing.

use std::collections::VecDeque;
use std::net::TcpStream;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Padding, Paragraph};
use ratatui::Frame;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::config::Config;
use crate::events::UiEvent;

const MAX_RECENT: usize = 8;
const ACCENT: Color = Color::LightRed;

struct RecentEntry {
    time: String,
    name: String,
    pages: usize,
    mode: String,
    token: String,
}

struct CurrentJob {
    name: String,
    pages_seen: usize,
}

struct DashState {
    jobs_captured: usize,
    pages_total: usize,
    recent: VecDeque<RecentEntry>,
    current: Option<CurrentJob>,
    mdns_ok: bool,
    server_up: bool,
    started_at: Instant,
    last_port_check: Option<Instant>,
}

impl DashState {
    fn new() -> Self {
        Self {
            jobs_captured: 0,
            pages_total: 0,
            recent: VecDeque::new(),
            current: None,
            mdns_ok: false,
            server_up: false,
            started_at: Instant::now(),
            last_port_check: None,
        }
    }

    fn apply(&mut self, event: UiEvent) {
        match event {
            UiEvent::JobStarted { name } => {
                let name = if name.is_empty() { "(no name)".to_string() } else { name };
                self.current = Some(CurrentJob { name, pages_seen: 0 });
            }
            UiEvent::JobProgress { pages_seen } => {
                if let Some(current) = &mut self.current {
                    current.pages_seen = pages_seen;
                }
            }
            UiEvent::JobDone { token, name, pages } => {
                let name = if name.is_empty() { "(no name)".to_string() } else { name };
                self.jobs_captured += 1;
                self.pages_total += pages;
                self.recent.push_front(RecentEntry {
                    time: chrono::Local::now().format("%H:%M:%S").to_string(),
                    name,
                    pages,
                    mode: "...".to_string(),
                    token,
                });
                self.recent.truncate(MAX_RECENT);
                self.current = None;
            }
            UiEvent::JobMode { token, mode } => {
                if let Some(entry) = self.recent.iter_mut().find(|e| e.token == token) {
                    entry.mode = mode;
                }
            }
            UiEvent::JobFailed => {
                self.current = None;
            }
            UiEvent::MdnsUp(ok) => {
                self.mdns_ok = ok;
            }
        }
    }

    fn refresh_port(&mut self, port: u16) {
        let now = Instant::now();
        let due = self
            .last_port_check
            .is_none_or(|t| now.duration_since(t) >= Duration::from_secs(1));
        if due {
            self.server_up = TcpStream::connect_timeout(
                &format!("127.0.0.1:{port}").parse().unwrap(),
                Duration::from_millis(400),
            )
            .is_ok();
            self.last_port_check = Some(now);
        }
    }
}

fn fmt_uptime(elapsed: Duration) -> String {
    let s = elapsed.as_secs();
    format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
}

fn ellipsis(s: &str, width: usize) -> String {
    if s.chars().count() <= width {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(width.saturating_sub(1)).collect();
        format!("{truncated}…")
    }
}

fn badge(ok: bool, up: &str, down: &str) -> Span<'static> {
    if ok {
        Span::styled(format!("● {up}"), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
    } else {
        Span::styled(format!("● {down}"), Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
    }
}

pub struct TuiContext {
    pub config: Config,
    pub ipp_url: String,
    pub printer_name: String,
}

fn draw_main(frame: &mut Frame, area: Rect, ctx: &TuiContext, state: &DashState) {
    let dim = Style::default().fg(Color::DarkGray);
    let mut lines: Vec<Line> = vec![
        Line::from(vec![
            Span::styled("    IPP  ", dim),
            Span::styled(ctx.ipp_url.clone(), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("  Spool  ", dim),
            Span::raw(ctx.config.spool_dir.display().to_string()),
        ]),
        Line::from(vec![
            Span::styled("  Inbox  ", dim),
            Span::raw(ctx.config.inbox_dir.display().to_string()),
        ]),
        Line::from(vec![
            Span::styled("   mDNS  ", dim),
            Span::raw(format!("{}  ", ctx.printer_name)),
            badge(state.mdns_ok, "advertised", "off"),
        ]),
        Line::from(vec![
            Span::styled(" Server  ", dim),
            Span::raw(format!(":{}  ", ctx.config.listen_port)),
            badge(state.server_up, "UP", "DOWN"),
            Span::raw(format!("    Uptime {}", fmt_uptime(state.started_at.elapsed()))),
        ]),
        Line::default(),
        Line::from(vec![
            Span::styled("Captured  ", dim),
            Span::styled(
                state.jobs_captured.to_string(),
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" jobs    ", dim),
            Span::styled(
                state.pages_total.to_string(),
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" pages", dim),
        ]),
        Line::default(),
    ];

    if let Some(current) = &state.current {
        lines.push(Line::from(vec![
            Span::styled("now", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw(format!("  rendering {}  ", ellipsis(&current.name, 24))),
            Span::styled(
                "█".repeat(12.min(current.pages_seen.max(1))),
                Style::default().fg(Color::Yellow),
            ),
            Span::raw(format!(" p{}", current.pages_seen)),
        ]));
    } else {
        lines.push(Line::from(Span::styled(
            "idle — waiting for a print job",
            dim.add_modifier(Modifier::ITALIC),
        )));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            " FAKE PRINTER ",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Line::from(Span::styled(" close this window to STOP ", dim)).centered())
        .padding(Padding::new(2, 2, 1, 1));
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn draw_connect(frame: &mut Frame, area: Rect) {
    let dim = Style::default().fg(Color::DarkGray);
    let lines = vec![
        Line::from(Span::styled("iPhone / Mac", Style::default().add_modifier(Modifier::BOLD))),
        Line::from(Span::styled("  AirPrint一覧に自動表示", dim)),
        Line::default(),
        Line::from(Span::styled("Windows", Style::default().add_modifier(Modifier::BOLD))),
        Line::from(Span::styled("  プリンター手動追加 → IPP", dim)),
        Line::from(Span::styled("  URLは左パネル", dim)),
    ];
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title("Connect")
        .padding(Padding::new(1, 1, 1, 1));
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn draw_recent(frame: &mut Frame, area: Rect, state: &DashState) {
    let dim = Style::default().fg(Color::DarkGray);
    let lines: Vec<Line> = if state.recent.is_empty() {
        vec![Line::from(Span::styled(
            "no jobs yet",
            dim.add_modifier(Modifier::ITALIC),
        ))]
    } else {
        state
            .recent
            .iter()
            .map(|r| {
                let mode_span = match r.mode.as_str() {
                    "text" => Span::styled("TEXT", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                    "png" => Span::styled("PNG ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                    _ => Span::styled("... ", dim),
                };
                Line::from(vec![
                    Span::styled(r.time.clone(), dim),
                    Span::raw(" "),
                    Span::raw(format!("{:<16}", ellipsis(&r.name, 16))),
                    Span::raw(" "),
                    mode_span,
                    Span::raw(format!(" {}p", r.pages)),
                ])
            })
            .collect()
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title("Recent activity")
        .padding(Padding::new(1, 1, 1, 1));
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn draw(frame: &mut Frame, ctx: &TuiContext, state: &DashState) {
    let dim = Style::default().fg(Color::DarkGray);
    let [top, foot] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)])
        .areas(frame.area());
    let [left, right] =
        Layout::horizontal([Constraint::Ratio(2, 3), Constraint::Ratio(1, 3)]).areas(top);
    let [connect, recent] =
        Layout::vertical([Constraint::Length(8), Constraint::Min(1)]).areas(right);

    draw_main(frame, left, ctx, state);
    draw_connect(frame, connect);
    draw_recent(frame, recent, state);

    let footer = Paragraph::new(Line::from(Span::styled(
        format!("inbox\\  ·  logs\\server.log   ·   {}", ctx.ipp_url),
        dim,
    )))
    .alignment(Alignment::Center);
    frame.render_widget(footer, foot);
}

/// Run the dashboard until the user quits (q / Ctrl+C / window close).
/// Errors if the terminal cannot enter raw mode (e.g. no console attached);
/// the caller falls back to headless operation.
pub fn run(ctx: TuiContext, mut rx: UnboundedReceiver<UiEvent>) -> std::io::Result<()> {
    let mut terminal = ratatui::try_init()?;
    let mut state = DashState::new();
    let result = loop {
        while let Ok(event) = rx.try_recv() {
            state.apply(event);
        }
        state.refresh_port(ctx.config.listen_port);

        if let Err(e) = terminal.draw(|frame| draw(frame, &ctx, &state)) {
            break Err(e);
        }

        match event::poll(Duration::from_millis(250)) {
            Ok(true) => {
                if let Ok(Event::Key(key)) = event::read() {
                    let ctrl_c = key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL);
                    if ctrl_c || key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
                        break Ok(());
                    }
                }
            }
            Ok(false) => {}
            Err(e) => break Err(e),
        }
    };
    ratatui::restore();
    result
}
