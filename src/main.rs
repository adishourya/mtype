use std::{
    env, fs, io, process,
    time::{Duration, Instant},
};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
    text::{Line, Span},
};


use rand::seq::SliceRandom;
use rand::thread_rng;

const DEFAULT_DURATION: u64 = 30;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode { Typing, Results }

#[derive(Clone, Copy, PartialEq, Eq)]
enum ContentMode { Text, Code }

struct Config {
    duration: u64,
    content: String,
    mode: ContentMode,
}

struct App {
    mode: Mode,
    duration: u64,
    target: Vec<char>,
    input: Vec<char>,
    start_time: Option<Instant>,
    end_time: Option<Instant>,
    content_mode: ContentMode,
}

fn main() -> io::Result<()> {
    let config = parse_args();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_app(&mut terminal, config);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    res
}

fn print_help() {
    println!(r#"
MTYPE: A Terminal Typing Test in Rust

USAGE:
    mtype [OPTIONS]

OPTIONS:
    -h          Print this help message
    -t <secs>   Set test duration (default: 30)
    -f <path>   Load text from a file (word-wraps)
    -c <path>   Load code from a file (skips indents/newlines)
    -w <count>  Limit the number of words from the source
"#);
}

fn parse_args() -> Config {
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_help();
        process::exit(0);
    }

    let mut duration = DEFAULT_DURATION;
    let mut content = "The quick brown fox jumps over the lazy dog. Pack my box with five dozen liquor jugs. Sphinx of black quartz, judge my vow. How vexingly quick daft zebras jump! Bright vixens jump; dozy fowl quack. Quick zephyrs blow, vexing daft Jim. Waltz, bad nymph, for quick jigs vex! Jived fox nymph grabs quick waltz. Glib jocks quiz nymph to vex dwarf. The five boxing wizards jump quickly. Jackdaws love my big sphinx of quartz. Quick wafting zephyrs vex bold Jim. Brawny gods just flocked up to quiz and vex him. Crazy Fredrick bought many very exquisite opal jewels. We promptly judged antique ivory buckles for the next prize. Amazingly few discotheques provide jukeboxes. ".repeat(5);
    let mut mode = ContentMode::Text;
    let mut word_limit: Option<usize> = None;

    if let Some(pos) = args.iter().position(|a| a == "-t") {
        duration = args.get(pos + 1).and_then(|v| v.parse().ok()).unwrap_or(DEFAULT_DURATION);
    }
    if let Some(pos) = args.iter().position(|a| a == "-f") {
        if let Some(path) = args.get(pos + 1) {
            content = fs::read_to_string(path).unwrap_or(content);
            mode = ContentMode::Text;
        }
    }
    if let Some(pos) = args.iter().position(|a| a == "-c") {
        if let Some(path) = args.get(pos + 1) {
            content = fs::read_to_string(path).unwrap_or(content);
            mode = ContentMode::Code;
        }
    }
    if let Some(pos) = args.iter().position(|a| a == "-w") {
        word_limit = args.get(pos + 1).and_then(|v| v.parse().ok());
    }

    if let Some(limit) = word_limit {
        content = content.split_whitespace().take(limit).collect::<Vec<_>>().join(" ");
    }

    Config { duration, content, mode }
}

impl App {
    fn new(config: Config) -> Self {

    let mut sentences: Vec<String> = config
        .content
        .split('.')
        .filter(|s| !s.trim().is_empty())
        .map(|s| format!("{}.", s.trim()))
        .collect();

    sentences.shuffle(&mut thread_rng());
    let shuffled_content = sentences.join(" ");

        Self {
            mode: Mode::Typing,
            duration: config.duration,
            target: shuffled_content.chars().collect(),
            input: vec![],
            start_time: None,
            end_time: None,
            content_mode: config.mode,
        }
    }

    fn time_left(&self) -> u64 {
        if let Some(end) = self.end_time {
            let elapsed = end.duration_since(self.start_time.unwrap()).as_secs();
            return self.duration.saturating_sub(elapsed);
        }
        self.start_time
            .map(|s| self.duration.saturating_sub(s.elapsed().as_secs()))
            .unwrap_or(self.duration)
    }

    fn current_wpm(&self) -> f64 {
        let now = self.end_time.unwrap_or_else(Instant::now);
        let elapsed = self.start_time.map(|s| now.duration_since(s).as_secs_f64()).unwrap_or(0.0).max(1.0);
        (self.input.len() as f64 / 5.0) / (elapsed / 60.0)
    }

    fn finished(&self) -> bool {
        self.time_left() == 0 || self.input.len() >= self.target.len()
    }

    fn progress(&self) -> (usize, usize) {
        let target_str: String = self.target.iter().collect();
        let total_words = target_str.split_whitespace().count();
        let input_str: String = self.input.iter().collect();
        let current_words = input_str.split_whitespace().count();
        (current_words, total_words)
    }

    // This function automatically skips spaces, tabs, and newlines in Code Mode
    fn push_input(&mut self, c: char) {
        if self.start_time.is_none() { self.start_time = Some(Instant::now()); }

        if self.input.len() < self.target.len() {
            self.input.push(c);

            // Auto-skip logic for Code Mode
            if self.content_mode == ContentMode::Code {
                while self.input.len() < self.target.len() {
                    let next_target = self.target[self.input.len()];
                    if next_target == '\n' || next_target == '\t' || (next_target == ' ' && (self.input.last() == Some(&'\n') || self.input.last() == Some(&' '))) {
                        self.input.push(next_target);
                    } else {
                        break;
                    }
                }
            }
        }
    }
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, config: Config) -> io::Result<()> {
    let mut app = App::new(config);

    loop {
        terminal.draw(|f| {
            match app.mode {
                Mode::Typing => draw_typing(f, &app),
                Mode::Results => draw_results(f, &app),
            }
        })?;

        if event::poll(Duration::from_millis(30))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match app.mode {
                        Mode::Typing => match key.code {
                            KeyCode::Char(c) => { app.push_input(c); }
                            KeyCode::Backspace => { app.input.pop(); }
                            KeyCode::Esc => return Ok(()),
                            _ => {}
                        },
                        Mode::Results => if key.code == KeyCode::Char('q') { return Ok(()); }
                    }
                }
            }
        }

        if app.mode == Mode::Typing && app.finished() {
            app.end_time = Some(Instant::now());
            app.mode = Mode::Results;
        }
    }
}

fn draw_typing(f: &mut Frame, app: &App) {
    let area = f.area();
    let horizontal_layout = Layout::horizontal([
        Constraint::Percentage(15),
        Constraint::Percentage(70),
        Constraint::Percentage(15),
    ]).split(area);
    let main_col = horizontal_layout[1];
    let vertical_layout = Layout::vertical([
        Constraint::Percentage(30),
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Length(3),
        Constraint::Min(0),
    ]).split(main_col);

    let (curr, total) = app.progress();
    let stats = Paragraph::new(format!(
        "WPM: {:.0}  |  TIME: {}s  |  WORDS: {}/{}",
        app.current_wpm(), app.time_left(), curr, total
    ))
    .alignment(Alignment::Center)
    .style(Style::default().fg(Color::Yellow));
    f.render_widget(stats, vertical_layout[1]);

    let width = vertical_layout[3].width as usize;
    let (lines, cursor_line_idx) = if app.content_mode == ContentMode::Code {
        wrap_code_mode(&app.target, &app.input, width)
    } else {
        wrap_text_mode(&app.target, &app.input, width)
    };

    let display_start_line = if cursor_line_idx > 0 { cursor_line_idx - 1 } else { 0 };

    let mut display_lines = Vec::new();
    for i in 0..3 {
        let line_idx = display_start_line + i;
        if let Some(line) = lines.get(line_idx) {
            let mut l = line.clone();
            if line_idx > cursor_line_idx {
                l = l.patch_style(Style::default().fg(Color::Rgb(60, 60, 60)));
            }
            display_lines.push(l);
        } else {
            display_lines.push(Line::from(""));
        }
    }

    let paragraph = Paragraph::new(display_lines)
        .alignment(if app.content_mode == ContentMode::Code { Alignment::Left } else { Alignment::Center });

    f.render_widget(paragraph, vertical_layout[3]);
}

fn wrap_text_mode<'a>(target: &[char], input: &[char], width: usize) -> (Vec<Line<'a>>, usize) {
    let mut lines = Vec::new();
    let mut current_line = Vec::new();
    let mut current_width = 0;
    let mut cursor_line_idx = 0;
    let mut global_char_idx = 0;

    let target_str: String = target.iter().collect();
    let words = target_str.split_inclusive(' ');

    for word in words {
        let word_chars: Vec<char> = word.chars().collect();
        if current_width + word_chars.len() > width && current_width > 0 {
            lines.push(Line::from(current_line.clone()));
            current_line.clear();
            current_width = 0;
        }
        for &c in &word_chars {
            if global_char_idx == input.len() { cursor_line_idx = lines.len(); }
            current_line.push(style_char(c, global_char_idx, input));
            global_char_idx += 1;
            current_width += 1;
        }
    }
    lines.push(Line::from(current_line));
    (lines, cursor_line_idx)
}

fn wrap_code_mode<'a>(target: &[char], input: &[char], width: usize) -> (Vec<Line<'a>>, usize) {
    let mut lines = Vec::new();
    let mut current_line = Vec::new();
    let mut current_width = 0;
    let mut cursor_line_idx = 0;

    for (idx, &c) in target.iter().enumerate() {
        if idx == input.len() { cursor_line_idx = lines.len(); }
        if c == '\n' {
            lines.push(Line::from(current_line.clone()));
            current_line.clear();
            current_width = 0;
        } else {
            current_line.push(style_char(c, idx, input));
            current_width += 1;
            if current_width >= width {
                lines.push(Line::from(current_line.clone()));
                current_line.clear();
                current_width = 0;
            }
        }
    }
    lines.push(Line::from(current_line));
    (lines, cursor_line_idx)
}

fn style_char<'a>(target_char: char, idx: usize, input: &[char]) -> Span<'a> {
    if idx < input.len() {
        if input[idx] == target_char {
            Span::styled(target_char.to_string(), Style::default().fg(Color::Green))
        } else {
            Span::styled(target_char.to_string(), Style::default().fg(Color::Red).bg(Color::Rgb(40, 0, 0)))
        }
    } else if idx == input.len() {
        Span::styled(target_char.to_string(), Style::default().bg(Color::White).fg(Color::Black))
    } else {
        Span::styled(target_char.to_string(), Style::default().fg(Color::DarkGray))
    }
}

fn calculate_accuracy(app: &App) -> f64 {
    if app.input.is_empty() { return 0.0; }
    let correct = app.input.iter().enumerate()
        .filter(|&(i, &c)| i < app.target.len() && c == app.target[i])
        .count();
    (correct as f64 / app.input.len() as f64) * 100.0
}

fn draw_results(f: &mut Frame, app: &App) {
    let rect = f.area();
    let stats = Paragraph::new(vec![
        Line::from(vec![Span::styled("TEST COMPLETE", Style::default().fg(Color::Green).bold())]),
        Line::from(""),
        Line::from(format!("Final WPM: {:.1}", app.current_wpm())),
        Line::from(format!("Accuracy: {:.1}%", calculate_accuracy(app))),
        Line::from(""),
        Line::from("Press 'q' to quit"),
    ])
    .alignment(Alignment::Center)
    .block(Block::default().borders(Borders::ALL));
    f.render_widget(stats, rect);
}
