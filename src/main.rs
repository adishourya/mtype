use std::{
    env, fs, io,
    time::{Duration, Instant},
};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Row, Table, TableState, Padding, Wrap},
    text::{Line, Span},
};

use rand::seq::SliceRandom;
use rand::{thread_rng, Rng};
use rusqlite::{Connection, params};
use chrono::Local;

const DEFAULT_DURATION: u64 = 30;
const DEFAULT_FILE: &str = "src/shakespear.txt";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode { Splash, Typing, Results, LayoutSetup }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ContentMode { Text, Code }

#[derive(Clone, Copy, PartialEq, Eq)]
enum SortOrder { Wpm, Latest }

#[derive(Clone)]
struct LayoutConfig {
    is_split: bool,
    rows: usize,
    cols: usize,
    keys: String,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            is_split: false,
            rows: 3,
            cols: 10,
            keys: "qwertyuiopasdfghjkl;zxcvbnm,./".to_string(),
        }
    }
}

struct Config {
    duration: u64,
    paragraphs: Vec<String>,
    mode: ContentMode,
}

struct App {
    mode: Mode,
    duration: u64,
    paragraphs: Vec<String>,
    target_text: String,
    input: Vec<char>,
    start_time: Option<Instant>,
    end_time: Option<Instant>,
    content_mode: ContentMode,
    db_conn: Connection,
    table_state: TableState,
    sort_order: SortOrder,
    history_cache: Vec<(String, String, String, String)>,
    layout: LayoutConfig,
    is_editing_layout: bool,
}

fn main() -> io::Result<()> {
    let config = parse_args();
    let conn = init_db().expect("Failed to initialize database");
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    let res = run_app(&mut terminal, config, conn);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    res
}

fn init_db() -> rusqlite::Result<Connection> {
    let mut db_path = home::home_dir().expect("Home dir not found");
    db_path.push(".mtype_stats.db");
    let conn = Connection::open(db_path)?;
    conn.execute("CREATE TABLE IF NOT EXISTS runs (id INTEGER PRIMARY KEY, wpm REAL, accuracy REAL, timestamp TEXT, mode TEXT DEFAULT 'Text')", [])?;
    conn.execute("CREATE TABLE IF NOT EXISTS layout (id INTEGER PRIMARY KEY, is_split INTEGER, rows INTEGER, cols INTEGER, keys TEXT)", [])?;
    Ok(conn)
}

fn save_layout(conn: &Connection, layout: &LayoutConfig) {
    let _ = conn.execute("INSERT OR REPLACE INTO layout (id, is_split, rows, cols, keys) VALUES (1, ?1, ?2, ?3, ?4)",
        params![layout.is_split as i32, layout.rows as i32, layout.cols as i32, layout.keys]);
}

fn load_layout(conn: &Connection) -> LayoutConfig {
    conn.query_row("SELECT is_split, rows, cols, keys FROM layout WHERE id = 1", [], |row| {
        Ok(LayoutConfig {
            is_split: row.get::<_, i32>(0)? != 0,
            rows: row.get::<_, i32>(1)? as usize,
            cols: row.get::<_, i32>(2)? as usize,
            keys: row.get::<_, String>(3)?,
        })
    }).unwrap_or_default()
}

fn save_run(conn: &Connection, wpm: f64, acc: f64, mode: ContentMode) {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let _ = conn.execute("INSERT INTO runs (wpm, accuracy, timestamp, mode) VALUES (?1, ?2, ?3, ?4)", params![wpm, acc, now, format!("{:?}", mode)]);
}

fn get_stats(conn: &Connection, sort: SortOrder) -> Vec<(String, String, String, String)> {
    let sql = match sort {
        SortOrder::Wpm => "SELECT wpm, accuracy, timestamp, mode FROM runs ORDER BY wpm DESC",
        SortOrder::Latest => "SELECT wpm, accuracy, timestamp, mode FROM runs ORDER BY id DESC",
    };
    let mut stmt = conn.prepare(sql).unwrap();
    stmt.query_map([], |row| Ok((format!("{:.1}", row.get::<_, f64>(0)?), format!("{:.1}%", row.get::<_, f64>(1)?), row.get::<_, String>(2)?, row.get::<_, String>(3)?)))
        .unwrap().filter_map(|r| r.ok()).collect()
}


// Replace your old DEFAULT_FILE and parse_args with this:

fn get_embedded_content() -> &'static str {
    // This embeds the file directly into your executable
    include_str!("shakespear.txt")
}

fn parse_args() -> Config {
    let args: Vec<String> = env::args().collect();
    let mut duration = DEFAULT_DURATION;
    let mut mode = ContentMode::Text;

    // 1. Get the content (either from a file argument or the embedded Shakespeare)
    let raw_content = if let Some(pos) = args.iter().position(|a| a == "-f") {
        let path = args.get(pos+1).cloned().unwrap_or_default();
        fs::read_to_string(path).unwrap_or_else(|_| get_embedded_content().to_string())
    } else if let Some(pos) = args.iter().position(|a| a == "-c") {
        mode = ContentMode::Code;
        let path = args.get(pos+1).cloned().unwrap_or_default();
        fs::read_to_string(path).unwrap_or_else(|_| get_embedded_content().to_string())
    } else {
        get_embedded_content().to_string()
    };

    // 2. Process into paragraphs

    // Inside parse_args(), where you process the paragraphs:
    let paragraphs = if mode == ContentMode::Text {
        raw_content.split("\n\n")
            .map(|p| {
                p.replace('\n', " ")
                 .replace('’', "'")  // Fix curly right quote
                 .replace('‘', "'")  // Fix curly left quote
                 .replace('”', "\"") // Fix curly right double quote
                 .replace('“', "\"") // Fix curly left double quote
                 .replace('—', "-")  // Fix em-dash to standard dash
                 .trim()
                 .to_string()
            })
            .filter(|p| p.len() > 20)
            .collect()
    } else {
        vec![raw_content]
    };

    if let Some(pos) = args.iter().position(|a| a == "-t") {
        duration = args.get(pos+1).and_then(|v| v.parse().ok()).unwrap_or(DEFAULT_DURATION);
    }

    Config { duration, paragraphs, mode }
}


impl App {
    fn new(config: Config, conn: Connection) -> Self {
        let layout = load_layout(&conn);
        let mut app = Self {
            mode: Mode::Splash,
            duration: config.duration,
            paragraphs: config.paragraphs.clone(),
            target_text: String::new(),
            input: vec![],
            start_time: None,
            end_time: None,
            content_mode: config.mode,
            db_conn: conn,
            table_state: TableState::default(),
            sort_order: SortOrder::Wpm,
            history_cache: vec![],
            layout,
            is_editing_layout: false
        };
        app.reset();
        app
    }

    fn reset(&mut self) {
        if !self.paragraphs.is_empty() {
            let full_text = self.paragraphs.choose(&mut thread_rng()).unwrap().clone();

            if self.content_mode == ContentMode::Text && full_text.len() > 150 {
                let mut rng = thread_rng();
                let max_start = full_text.len().saturating_sub(150);
                let mut start_idx = rng.gen_range(0..max_start);

                if let Some(next_space) = full_text[start_idx..].find(' ') {
                    start_idx += next_space + 1;
                }
                self.target_text = full_text[start_idx..].trim().to_string();
            } else {
                self.target_text = full_text;
            }
        }
        self.input = vec![];
        self.start_time = None;
        self.end_time = None;
        self.refresh_history();
    }

    fn refresh_history(&mut self) {
        self.history_cache = get_stats(&self.db_conn, self.sort_order);
        if !self.history_cache.is_empty() && self.table_state.selected().is_none() { self.table_state.select(Some(0)); }
    }

    fn current_wpm(&self) -> f64 {
        let elapsed = self.start_time.map(|s| self.end_time.unwrap_or_else(Instant::now).duration_since(s).as_secs_f64()).unwrap_or(0.0).max(1.0);
        (self.input.len() as f64 / 5.0) / (elapsed / 60.0)
    }

    fn next_history(&mut self) {
        let i = match self.table_state.selected() {
            Some(i) => if i >= self.history_cache.len().saturating_sub(1) { 0 } else { i + 1 },
            None => 0,
        };
        self.table_state.select(Some(i));
    }

    fn prev_history(&mut self) {
        let i = match self.table_state.selected() {
            Some(i) => if i == 0 { self.history_cache.len().saturating_sub(1) } else { i - 1 },
            None => 0,
        };
        self.table_state.select(Some(i));
    }

    fn push_input(&mut self, c: char) {
        if self.start_time.is_none() { self.start_time = Some(Instant::now()); }
        let target_chars: Vec<char> = self.target_text.chars().collect();
        if self.input.len() < target_chars.len() {
            self.input.push(c);
            if c == '\n' && self.content_mode == ContentMode::Code {
                let remaining = &target_chars[self.input.len()..];
                for &next_c in remaining {
                    if next_c == ' ' || next_c == '\t' { self.input.push(next_c); }
                    else { break; }
                }
            }
        }
    }
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, config: Config, conn: Connection) -> io::Result<()> {
    let mut app = App::new(config, conn);
    loop {
        terminal.draw(|f| match app.mode {
            Mode::Splash => draw_splash(f),
            Mode::Typing => draw_typing(f, &app),
            Mode::Results => draw_results(f, &mut app),
            Mode::LayoutSetup => draw_layout_setup(f, &app),
        })?;

        if event::poll(Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match (app.mode, key.code) {
                        (Mode::Splash, KeyCode::Enter) => app.mode = Mode::Typing,
                        (Mode::Splash, KeyCode::Char('l')) => app.mode = Mode::LayoutSetup,
                        (Mode::Splash, KeyCode::Esc) => return Ok(()),
                        (Mode::LayoutSetup, _) => handle_layout_keys(&mut app, key.code),
                        (Mode::Typing, KeyCode::Esc) => { app.reset(); app.mode = Mode::Splash; }
                        (Mode::Typing, KeyCode::Backspace) => { app.input.pop(); }
                        (Mode::Typing, KeyCode::Enter) => app.push_input('\n'),
                        (Mode::Typing, KeyCode::Char(c)) => app.push_input(c),
                        (Mode::Results, KeyCode::Esc) | (Mode::Results, KeyCode::Enter) => { app.reset(); app.mode = Mode::Splash; }
                        (Mode::Results, KeyCode::Down) | (Mode::Results, KeyCode::Char('j')) => app.next_history(),
                        (Mode::Results, KeyCode::Up) | (Mode::Results, KeyCode::Char('k')) => app.prev_history(),
                        (Mode::Results, KeyCode::Char('t')) => {
                            app.sort_order = if app.sort_order == SortOrder::Wpm { SortOrder::Latest } else { SortOrder::Wpm };
                            app.refresh_history();
                        }
                        _ => {}
                    }
                }
            }
        }
        if app.mode == Mode::Typing && (app.input.len() >= app.target_text.len() || app.start_time.map(|s| s.elapsed().as_secs() >= app.duration).unwrap_or(false)) {
            app.end_time = Some(Instant::now());
            save_run(&app.db_conn, app.current_wpm(), calculate_accuracy(&app), app.content_mode);
            app.refresh_history();
            app.mode = Mode::Results;
        }
    }
}

fn handle_layout_keys(app: &mut App, code: KeyCode) {
    if !app.is_editing_layout {
        match code {
            KeyCode::Char('e') => app.is_editing_layout = true,
            KeyCode::Char('s') => app.layout.is_split = !app.layout.is_split,
            KeyCode::Up => app.layout.rows = (app.layout.rows + 1).min(6),
            KeyCode::Down => app.layout.rows = app.layout.rows.saturating_sub(1).max(1),
            KeyCode::Right => app.layout.cols = (app.layout.cols + 1).min(20),
            KeyCode::Left => app.layout.cols = app.layout.cols.saturating_sub(1).max(1),
            KeyCode::Esc => { save_layout(&app.db_conn, &app.layout); app.mode = Mode::Splash; }
            _ => {}
        }
    } else {
        match code {
            KeyCode::Esc => app.is_editing_layout = false,
            KeyCode::Backspace => { app.layout.keys.pop(); }
            KeyCode::Char(c) => if app.layout.keys.len() < (app.layout.rows * app.layout.cols) { app.layout.keys.push(c); }
            _ => {}
        }
    }
}

fn draw_splash(f: &mut Frame) {
    let chunks = Layout::vertical([Constraint::Percentage(30), Constraint::Length(10), Constraint::Min(0)]).split(f.area());
    f.render_widget(Paragraph::new(vec![Line::from(" M T Y P E ").bold().magenta(), Line::from("Terminal Speed Typing").italic().gray()]).alignment(Alignment::Center), chunks[1]);
    let menu = Paragraph::new(vec![
        Line::from(vec![Span::raw("ENTER   "), Span::styled("Start", Style::default().green())]),
        Line::from(vec![Span::raw("L        "), Span::styled("Layout Setup", Style::default().cyan())]),
        Line::from(vec![Span::raw("ESC      "), Span::styled("Quit", Style::default().red())]),
    ]);
    let horiz = Layout::horizontal([Constraint::Fill(1), Constraint::Length(40), Constraint::Fill(1)]).split(chunks[1]);
    let mut area = horiz[1]; area.y += 3;
    f.render_widget(menu, area);
}

fn draw_typing(f: &mut Frame, app: &App) {
    let kb_height = (app.layout.rows as u16) + 2;
    let chunks = Layout::vertical([Constraint::Length(1), Constraint::Min(3), Constraint::Length(kb_height)]).split(f.area());
    let time_left = app.start_time.map(|s| app.duration.saturating_sub(s.elapsed().as_secs())).unwrap_or(app.duration);
    f.render_widget(Paragraph::new(format!("WPM: {:.0} | TIME: {}s", app.current_wpm(), time_left)).alignment(Alignment::Right), chunks[0]);

    let target_chars: Vec<char> = app.target_text.chars().collect();
    let mut lines = Vec::new();
    let mut current_line = Vec::new();

    for (i, &c) in target_chars.iter().enumerate() {
        if c == '\n' {
            lines.push(Line::from(current_line.clone()));
            current_line.clear();
        } else {
            current_line.push(style_char(c, i, &app.input));
        }
    }
    lines.push(Line::from(current_line));

    let block = Block::default().padding(Padding::horizontal(2)).borders(Borders::NONE);
    let mut p = Paragraph::new(lines).block(block);
    if app.content_mode == ContentMode::Text { p = p.wrap(Wrap { trim: true }); }

    f.render_widget(p, chunks[1]);
    render_keyboard_visualization(f, chunks[2], &app.layout, &app.target_text, app.input.len());
}

fn style_char<'a>(target: char, idx: usize, input: &[char]) -> Span<'a> {
    let input_len = input.len();
    if idx < input_len {
        // PERMANENT history colors
        let style = if input[idx] == target { Style::default().fg(Color::Green) } else { Style::default().fg(Color::Red) };
        Span::styled(target.to_string(), style)
    } else if idx == input_len {
        // CURRENT character bright yellow
        let display = if target == '\n' { "↵".to_string() } else { target.to_string() };
        Span::styled(display, Style::default().fg(Color::LightYellow))
    } else if idx == input_len + 1 {
        // NEXT character (Yellow)
        Span::styled(target.to_string(), Style::default().fg(Color::Indexed(136)))
    } else {
        // FUTURE text
        Span::styled(target.to_string(), Style::default().fg(Color::Indexed(240)))
    }
}

fn render_keyboard_visualization(f: &mut Frame, area: Rect, layout: &LayoutConfig, target_text: &str, input_pos: usize) {
    let mut rows = Vec::new();
    let keys: Vec<char> = layout.keys.chars().collect();
    let chars: Vec<char> = target_text.chars().collect();

    let current_char = chars.get(input_pos);
    let next_char = chars.get(input_pos + 1);

    for r in 0..layout.rows {
        let mut spans = Vec::new();
        for c in 0..layout.cols {
            let idx = r * layout.cols + c;
            let key_char = keys.get(idx).unwrap_or(&' ');
            let mut style = Style::default().fg(Color::Indexed(245));
            let key_low = key_char.to_lowercase().next().unwrap();

            // Next-to-next is Yellow
            if let Some(&nc) = next_char {
                let nc_low = if nc == '\n' { '↵' } else { nc.to_lowercase().next().unwrap() };
                if key_low == nc_low { style = style.fg(Color::Indexed(136)); }
            }
            // Immediate target is White (takes priority)
            if let Some(&cc) = current_char {
                let cc_low = if cc == '\n' { '↵' } else { cc.to_lowercase().next().unwrap() };
                if key_low == cc_low { style = style.fg(Color::LightYellow).bold(); }
            }

            spans.push(Span::styled(format!("[{}]", key_char), style));
            if layout.is_split && c == (layout.cols / 2) - 1 { spans.push(Span::raw("    ")); }
        }
        rows.push(Line::from(spans));
    }
    f.render_widget(Paragraph::new(rows).alignment(Alignment::Center).block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray))), area);
}

fn draw_results(f: &mut Frame, app: &mut App) {
    let chunks = Layout::vertical([Constraint::Percentage(15), Constraint::Percentage(85)]).split(f.area());
    let summary = Paragraph::new(vec![
        Line::from(vec![Span::styled("COMPLETE ", Style::default().bold().green()), Span::raw(format!("| WPM: {:.1} | ACC: {:.1}%", app.current_wpm(), calculate_accuracy(app)))]),
        Line::from(vec![Span::styled(" [T]", Style::default().yellow()), Span::raw(" Toggle Sort | "), Span::styled("[J/K]", Style::default().yellow()), Span::raw(" Scroll | "), Span::styled("[ESC]", Style::default().yellow()), Span::raw(" Home")])
    ]).alignment(Alignment::Center).block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray)));
    f.render_widget(summary, chunks[0]);
    let rows: Vec<Row> = app.history_cache.iter().map(|(w, a, d, m)| Row::new(vec![w.clone(), a.clone(), m.clone(), d.clone()])).collect();
    let table = Table::new(rows, [Constraint::Percentage(15), Constraint::Percentage(15), Constraint::Percentage(15), Constraint::Percentage(55)])
        .header(Row::new(vec!["WPM", "ACC", "MODE", "DATE"]).style(Style::default().bold().yellow()))
        .block(Block::default().borders(Borders::ALL).title(" History ").border_style(Style::default().fg(Color::DarkGray)))
        .row_highlight_style(Style::default().bg(Color::Rgb(40, 40, 40))).highlight_symbol(">> ");
    f.render_stateful_widget(table, chunks[1], &mut app.table_state);
}

fn draw_layout_setup(f: &mut Frame, app: &App) {
    let kb_height = (app.layout.rows as u16) + 2;
    let chunks = Layout::vertical([Constraint::Length(5), Constraint::Length(kb_height)]).split(f.area());
    f.render_widget(Paragraph::new(format!("MODE: {} | S: Split | Arrows: Size | E: Edit keys", if app.is_editing_layout { "EDIT" } else { "COMMAND" })).block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray))), chunks[0]);
    render_keyboard_visualization(f, chunks[1], &app.layout, "", 0);
}

fn calculate_accuracy(app: &App) -> f64 {
    if app.input.is_empty() { return 0.0; }
    let target: Vec<char> = app.target_text.chars().collect();
    let correct = app.input.iter().enumerate().filter(|&(i, &c)| i < target.len() && c == target[i]).count();
    (correct as f64 / app.input.len() as f64) * 100.0
}
