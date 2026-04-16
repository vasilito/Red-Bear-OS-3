use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};

use crate::app::{App, Field, Focus};
use crate::backend::{ConsoleBackend, SecurityKind};

struct Palette {
    title: Color,
    accent: Color,
    selected: Color,
    success: Color,
    danger: Color,
    muted: Color,
}

const PALETTE: Palette = Palette {
    title: Color::Cyan,
    accent: Color::Yellow,
    selected: Color::LightYellow,
    success: Color::Green,
    danger: Color::Red,
    muted: Color::DarkGray,
};

pub fn render<B: ConsoleBackend>(frame: &mut ratatui::Frame, app: &App<B>) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(18),
            Constraint::Length(5),
        ])
        .split(frame.area());

    render_header(frame, app, layout[0]);
    render_body(frame, app, layout[1]);
    render_footer(frame, app, layout[2]);

    if app.editor.is_some() {
        render_editor(frame, app);
    }
}

fn render_header<B: ConsoleBackend>(frame: &mut ratatui::Frame, app: &App<B>, area: Rect) {
    let mut status_spans = vec![Span::styled(
        " Red Bear Wi-Fi Console ",
        Style::default()
            .fg(PALETTE.title)
            .add_modifier(Modifier::BOLD),
    )];

    if let Some(active) = &app.active_profile {
        status_spans.push(Span::raw("  "));
        status_spans.push(Span::styled(
            format!("active={active}"),
            Style::default().fg(PALETTE.success),
        ));
    }

    status_spans.push(Span::raw("  "));
    status_spans.push(Span::styled(
        format!("interface={}", app.status.interface),
        Style::default().fg(PALETTE.accent),
    ));

    if app.dirty {
        status_spans.push(Span::raw("  "));
        status_spans.push(Span::styled(
            "unsaved changes",
            Style::default().fg(PALETTE.danger),
        ));
    }

    let paragraph = Paragraph::new(vec![Line::from(status_spans)])
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(paragraph, area);
}

fn render_body<B: ConsoleBackend>(frame: &mut ratatui::Frame, app: &App<B>, area: Rect) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(28),
            Constraint::Length(34),
            Constraint::Min(34),
        ])
        .split(area);

    render_profiles(frame, app, columns[0]);

    let middle = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(12), Constraint::Min(6)])
        .split(columns[1]);
    render_status(frame, app, middle[0]);
    render_scan(frame, app, middle[1]);
    render_editor_fields(frame, app, columns[2]);
}

fn render_profiles<B: ConsoleBackend>(frame: &mut ratatui::Frame, app: &App<B>, area: Rect) {
    let items = if app.profiles.is_empty() {
        vec![ListItem::new("No Wi-Fi profiles yet")]
    } else {
        app.profiles
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let mut line = name.clone();
                if app.active_profile.as_deref() == Some(name.as_str()) {
                    line = format!("* {line}");
                }

                let style = if index == app.selected_profile {
                    selected_style(app.focus == Focus::Profiles)
                } else {
                    Style::default()
                };

                ListItem::new(line).style(style)
            })
            .collect::<Vec<_>>()
    };

    frame.render_widget(
        List::new(items).block(
            Block::default()
                .title("Profiles")
                .borders(Borders::ALL)
                .border_style(border_style(app.focus == Focus::Profiles)),
        ),
        area,
    );
}

fn render_status<B: ConsoleBackend>(frame: &mut ratatui::Frame, app: &App<B>, area: Rect) {
    let lines = vec![
        kv_line("Address", &app.status.address),
        kv_line("Status", &app.status.status),
        kv_line("Link", &app.status.link_state),
        kv_line("Firmware", &app.status.firmware_status),
        kv_line("Transport", &app.status.transport_status),
        kv_line("Init", &app.status.transport_init_status),
        kv_line("Activation", &app.status.activation_status),
        kv_line("Connect", &app.status.connect_result),
        kv_line("Disconnect", &app.status.disconnect_result),
        kv_line("Last error", &app.status.last_error),
    ];

    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title("Live Status").borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_scan<B: ConsoleBackend>(frame: &mut ratatui::Frame, app: &App<B>, area: Rect) {
    let items = if app.scans.is_empty() {
        vec![ListItem::new("Press r to scan the selected interface")]
    } else {
        app.scans
            .iter()
            .enumerate()
            .map(|(index, scan)| {
                let style = if index == app.selected_scan {
                    selected_style(app.focus == Focus::Scan)
                } else {
                    Style::default()
                };
                ListItem::new(scan.label()).style(style)
            })
            .collect::<Vec<_>>()
    };

    frame.render_widget(
        List::new(items).block(
            Block::default()
                .title("Scan Results")
                .borders(Borders::ALL)
                .border_style(border_style(app.focus == Focus::Scan)),
        ),
        area,
    );
}

fn render_editor_fields<B: ConsoleBackend>(frame: &mut ratatui::Frame, app: &App<B>, area: Rect) {
    let selected = app.selected_field();
    let rows = app
        .visible_fields()
        .into_iter()
        .map(|field| render_field_line(app, field, field == selected))
        .collect::<Vec<_>>();

    frame.render_widget(
        Paragraph::new(rows)
            .block(
                Block::default()
                    .title("Profile Draft")
                    .borders(Borders::ALL)
                    .border_style(border_style(app.focus == Focus::Fields)),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_footer<B: ConsoleBackend>(frame: &mut ratatui::Frame, app: &App<B>, area: Rect) {
    let message_style = if app.message.starts_with("Error:") {
        Style::default().fg(PALETTE.danger)
    } else {
        Style::default()
    };

    let security_note = match app.draft.security {
        SecurityKind::Open => "open network selected",
        SecurityKind::Wpa2Psk => "wpa2-psk key required",
    };

    let lines = vec![
        Line::from(vec![
            Span::styled("Message: ", Style::default().fg(PALETTE.accent)),
            Span::styled(app.message.clone(), message_style),
        ]),
        Line::from(vec![
            Span::styled("Keys: ", Style::default().fg(PALETTE.accent)),
            Span::raw(
                "Tab focus  Enter load/apply/edit  r scan  s save  a activate  c connect  d disconnect  n new  q quit",
            ),
        ]),
        Line::from(vec![
            Span::styled("Hints: ", Style::default().fg(PALETTE.accent)),
            Span::styled(security_note, Style::default().fg(PALETTE.muted)),
            Span::raw("  •  "),
            Span::styled(
                "connect uses /scheme/wifictl plus /etc/netctl persistence",
                Style::default().fg(PALETTE.muted),
            ),
        ]),
    ];

    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title("Console Flow").borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_editor<B: ConsoleBackend>(frame: &mut ratatui::Frame, app: &App<B>) {
    let Some(editor) = &app.editor else {
        return;
    };

    let area = centered_rect(frame.area(), 72, 22);
    let lines = vec![
        Line::from(vec![Span::styled(
            format!("Editing {}", editor.field.label()),
            Style::default()
                .fg(PALETTE.title)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        Line::from(editor.buffer.clone()),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Enter saves • Esc cancels • Backspace deletes",
            Style::default().fg(PALETTE.muted),
        )]),
    ];

    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title("Field Editor").borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_field_line<B: ConsoleBackend>(
    app: &App<B>,
    field: Field,
    selected: bool,
) -> Line<'static> {
    let label_style = if selected && app.focus == Focus::Fields {
        Style::default()
            .fg(PALETTE.selected)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(PALETTE.accent)
    };

    let marker = if selected { ">" } else { " " };
    Line::from(vec![
        Span::styled(format!("{marker} {:<12}", field.label()), label_style),
        Span::raw(app.field_value(field)),
    ])
}

fn kv_line(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<10} "), Style::default().fg(PALETTE.accent)),
        Span::raw(value.to_string()),
    ])
}

fn border_style(active: bool) -> Style {
    if active {
        Style::default().fg(PALETTE.selected)
    } else {
        Style::default()
    }
}

fn selected_style(active: bool) -> Style {
    if active {
        Style::default()
            .fg(Color::Black)
            .bg(PALETTE.selected)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(PALETTE.selected)
    }
}

fn centered_rect(area: Rect, width_percent: u16, height_percent: u16) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - height_percent) / 2),
            Constraint::Percentage(height_percent),
            Constraint::Percentage((100 - height_percent) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_percent) / 2),
            Constraint::Percentage(width_percent),
            Constraint::Percentage((100 - width_percent) / 2),
        ])
        .split(vertical[1])[1]
}
