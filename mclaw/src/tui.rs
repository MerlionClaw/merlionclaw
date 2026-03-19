//! Terminal UI dashboard for MerlionClaw.

use std::io;
use std::path::Path;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Terminal;

use crate::AppConfig;

/// Data collected for the dashboard display.
struct DashboardData {
    gateway_addr: String,
    model: String,
    total_tools: usize,
    /// (name, description, version, tool_count)
    skills: Vec<(String, String, String, usize)>,
    channels: Vec<(String, bool)>,
    fact_count: usize,
}

fn collect_data(config: &AppConfig) -> DashboardData {
    let gateway_addr = format!("{}:{}", config.gateway.host, config.gateway.port);
    let model = config.agent.default_model.clone();

    // Discover skills
    let skills_dir = Path::new("skills");
    let (skills, total_tools) = match mclaw_skills::registry::SkillRegistry::discover(skills_dir) {
        Ok(registry) => {
            let info = registry.skill_info();
            let total: usize = info.iter().map(|(_, _, _, count)| count).sum();
            (info, total)
        }
        Err(_) => (vec![], 0),
    };

    // Channels
    let channels = vec![
        ("Telegram".to_string(), config.channels.telegram.enabled),
        ("Slack".to_string(), config.channels.slack.enabled),
        ("Discord".to_string(), config.channels.discord.enabled),
        ("WhatsApp".to_string(), config.channels.whatsapp.enabled),
        ("Teams".to_string(), config.channels.teams.enabled),
    ];

    // Memory fact count
    let memory_dir = shellexpand::tilde(&config.memory.dir).to_string();
    let facts_file = Path::new(&memory_dir).join("facts.md");
    let fact_count = if facts_file.exists() {
        std::fs::read_to_string(&facts_file)
            .map(|content| content.lines().filter(|l| l.starts_with("- ")).count())
            .unwrap_or(0)
    } else {
        0
    };

    DashboardData {
        gateway_addr,
        model,
        total_tools,
        skills,
        channels,
        fact_count,
    }
}

/// Run the TUI dashboard. Blocks until the user presses 'q'.
pub fn run(config: &AppConfig) -> anyhow::Result<()> {
    let data = collect_data(config);

    enable_raw_mode()?;
    crossterm::execute!(io::stdout(), EnterAlternateScreen)?;
    let backend = ratatui::backend::CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    loop {
        terminal.draw(|f| draw(f, &data))?;

        if event::poll(std::time::Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => break,
                        KeyCode::Char('r') | KeyCode::Char('R') => {
                            // Refresh is a no-op for now; data was collected once.
                            // Could re-collect here if needed.
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    disable_raw_mode()?;
    crossterm::execute!(io::stdout(), LeaveAlternateScreen)?;
    Ok(())
}

fn draw(f: &mut ratatui::Frame, data: &DashboardData) {
    let size = f.area();

    // Vertical layout: header, body, footer
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // header
            Constraint::Min(10),   // body
            Constraint::Length(3), // footer
        ])
        .split(size);

    // Header
    let version = env!("CARGO_PKG_VERSION");
    let header = Paragraph::new(Line::from(vec![
        Span::styled(
            " MerlionClaw Dashboard ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("v{version}"),
            Style::default().fg(Color::DarkGray),
        ),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );
    f.render_widget(header, outer[0]);

    // Body: split into two columns
    let body_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(outer[1]);

    // Left column: status + channels
    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(7), Constraint::Min(4)])
        .split(body_cols[0]);

    // Status panel
    let status_lines = vec![
        Line::from(vec![
            Span::styled("  Gateway: ", Style::default().fg(Color::Gray)),
            Span::styled(&data.gateway_addr, Style::default().fg(Color::Green)),
        ]),
        Line::from(vec![
            Span::styled("  Model:   ", Style::default().fg(Color::Gray)),
            Span::styled(&data.model, Style::default().fg(Color::Green)),
        ]),
        Line::from(vec![
            Span::styled("  Skills:  ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{}", data.skills.len()),
                Style::default().fg(Color::Green),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Tools:   ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{}", data.total_tools),
                Style::default().fg(Color::Green),
            ),
        ]),
    ];
    let status = Paragraph::new(status_lines).block(
        Block::default()
            .title(" Status ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );
    f.render_widget(status, left[0]);

    // Channels panel
    let channel_lines: Vec<Line> = data
        .channels
        .iter()
        .map(|(name, enabled)| {
            let (indicator, color) = if *enabled {
                ("ON ", Color::Green)
            } else {
                ("OFF", Color::DarkGray)
            };
            Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("[{indicator}]"), Style::default().fg(color)),
                Span::raw(format!(" {name}")),
            ])
        })
        .collect();
    let channels = Paragraph::new(channel_lines).block(
        Block::default()
            .title(" Channels ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );
    f.render_widget(channels, left[1]);

    // Right column: skills + memory
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(6), Constraint::Length(5)])
        .split(body_cols[1]);

    // Skills panel
    let skill_lines: Vec<Line> = if data.skills.is_empty() {
        vec![Line::from(Span::styled(
            "  No skills discovered",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        data.skills
            .iter()
            .map(|(name, desc, version, tool_count)| {
                Line::from(vec![
                    Span::styled(
                        format!("  {name}"),
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!(" v{version}"),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(
                        format!(" ({tool_count} tools) "),
                        Style::default().fg(Color::Gray),
                    ),
                    Span::styled(desc, Style::default().fg(Color::White)),
                ])
            })
            .collect()
    };
    let skills = Paragraph::new(skill_lines).block(
        Block::default()
            .title(" Skills ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );
    f.render_widget(skills, right[0]);

    // Memory panel
    let memory_lines = vec![
        Line::from(vec![
            Span::styled("  Facts: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{}", data.fact_count),
                Style::default().fg(Color::Green),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Dir:   ", Style::default().fg(Color::Gray)),
            Span::styled("~/.merlionclaw/memory", Style::default().fg(Color::White)),
        ]),
    ];
    let memory = Paragraph::new(memory_lines).block(
        Block::default()
            .title(" Memory ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );
    f.render_widget(memory, right[1]);

    // Footer
    let footer = Paragraph::new(Line::from(vec![
        Span::styled(
            " q",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(": quit", Style::default().fg(Color::Gray)),
        Span::styled("  |  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            "r",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(": refresh", Style::default().fg(Color::Gray)),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    f.render_widget(footer, outer[2]);
}
