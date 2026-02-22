use std::{env, io, time::{Duration, Instant}};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

use ratatui::{
    prelude::*,
    widgets::{Axis, Block, Borders, Chart, Dataset, GraphType, Paragraph},
    text::Line,
};

const DEFAULT_DURATION: u64 = 15;
const TEXT: &str = "Lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor incididunt ut labore";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Typing,
    Results,
}

struct App {
    mode: Mode,
    duration: u64,
    target: Vec<char>,
    input: Vec<char>,
    start_time: Option<Instant>,
    end_time: Option<Instant>,
    wpm_history: Vec<(f64, f64)>,
    last_sample: Instant,
}

fn main() -> io::Result<()> {
    let duration = parse_duration();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_app(&mut terminal, duration);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    res
}

fn parse_duration() -> u64 {
    let args: Vec<String> = env::args().collect();
    if let Some(pos) = args.iter().position(|a| a == "-t") {
        args.get(pos + 1)
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_DURATION)
    } else {
        DEFAULT_DURATION
    }
}

impl App {
    fn new(duration: u64) -> Self {
        Self {
            mode: Mode::Typing,
            duration,
            target: TEXT.chars().collect(),
            input: vec![],
            start_time: None,
            end_time: None,
            wpm_history: vec![],
            last_sample: Instant::now(),
        }
    }

    fn elapsed(&self) -> f64 {
        match self.start_time {
            Some(start) => {
                let end = self.end_time.unwrap_or_else(Instant::now);
                (end - start).as_secs_f64()
            }
            None => 0.0,
        }
    }

    fn time_left(&self) -> u64 {
        if let Some(start) = self.start_time {
            self.duration.saturating_sub(start.elapsed().as_secs())
        } else {
            self.duration
        }
    }

    fn current_wpm(&self) -> f64 {
        let elapsed = self.elapsed().max(1.0);
        (self.input.len() as f64 / 5.0) / (elapsed / 60.0)
    }

    fn finished(&self) -> bool {
        self.time_left() == 0 || self.input.len() == self.target.len()
    }
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    duration: u64,
) -> io::Result<()> {
    let mut app = App::new(duration);

    loop {
        terminal.draw(|f| match app.mode {
            Mode::Typing => draw_typing(f, &app),
            Mode::Results => draw_results(f, &app),
        })?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match app.mode {
                        Mode::Typing => match key.code {
                            KeyCode::Char(c) => {
                                if app.start_time.is_none() {
                                    app.start_time = Some(Instant::now());
                                }
                                if app.input.len() < app.target.len() {
                                    app.input.push(c);
                                }
                            }
                            KeyCode::Backspace => {
                                app.input.pop();
                            }
                            KeyCode::Esc => return Ok(()),
                            _ => {}
                        },
                        Mode::Results => {
                            if key.code == KeyCode::Char('q') {
                                return Ok(());
                            }
                        }
                    }
                }
            }
        }

        if matches!(app.mode, Mode::Typing) {
            if app.start_time.is_some()
                && app.last_sample.elapsed() >= Duration::from_millis(500)
            {
                let t = app.elapsed();
                let wpm = app.current_wpm();
                app.wpm_history.push((t, wpm));
                app.last_sample = Instant::now();
            }

            if app.finished() {
                app.end_time = Some(Instant::now());
                app.mode = Mode::Results;
            }
        }
    }
}

fn draw_typing(f: &mut Frame, app: &App) {
    let size = f.area();

    let vertical = Layout::vertical([
        Constraint::Percentage(40),
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Percentage(40),
    ])
    .split(size);

    let timer = Paragraph::new(format!("{}s", app.time_left()))
        .style(Style::default().fg(Color::Blue))
        .alignment(Alignment::Center);

    f.render_widget(timer, vertical[1]);

    let mut spans = vec![];

    for (i, &c) in app.target.iter().enumerate() {
        let style = if i < app.input.len() {
            if app.input[i] == c {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::Red)
            }
        } else {
            Style::default().fg(Color::DarkGray)
        };

        spans.push(Span::styled(c.to_string(), style));
    }



    // simulate caret
    if app.input.len() < app.target.len() {
        spans[app.input.len()].style = spans[app.input.len()].style.bg(Color::White);
    }

    let text = Paragraph::new(Line::from(spans)).alignment(Alignment::Center);
    f.render_widget(text, vertical[2]);
}

fn draw_results(f: &mut Frame, app: &App) {
    let size = f.area();

    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(10),
        Constraint::Length(2),
    ])
    .split(size);

    let wpm = app.current_wpm();

    let stats = Paragraph::new(format!("Final WPM: {:.2}", wpm))
        .alignment(Alignment::Center);

    f.render_widget(stats, chunks[0]);

    let max_time = app.wpm_history.last().map(|p| p.0).unwrap_or(1.0);
    let max_wpm = app.wpm_history.iter().map(|p| p.1).fold(0.0, f64::max) + 10.0;

    let dataset = Dataset::default()
        .graph_type(GraphType::Line)
        .data(&app.wpm_history)
        .style(Style::default().fg(Color::Yellow));

    let chart = Chart::new(vec![dataset])
        .block(Block::default().title("WPM over time").borders(Borders::ALL))
        .x_axis(
            Axis::default()
                .title("Time")
                .bounds([0.0, max_time])
                .labels(vec![
                    Line::from("0"),
                    Line::from(format!("{:.0}", max_time)),
                ]),
        )
        .y_axis(
            Axis::default()
                .title("WPM")
                .bounds([0.0, max_wpm])
                .labels(vec![
                    Line::from("0"),
                    Line::from(format!("{:.0}", max_wpm)),
                ]),
        );

    f.render_widget(chart, chunks[1]);

    let hint = Paragraph::new("Press q to quit").alignment(Alignment::Center);
    f.render_widget(hint, chunks[2]);
}
