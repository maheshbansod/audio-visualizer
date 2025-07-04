use std::{
    fmt::Display,
    path::{Path, PathBuf},
    str::FromStr,
    sync::mpsc,
    time::{Duration, Instant},
};

use color_eyre::eyre::{Error, Result};
use ratatui::{
    DefaultTerminal, Frame,
    crossterm::event::{self, Event, KeyCode},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span, Text},
    widgets::{Axis, Block, Chart, Dataset, Paragraph},
};

use crate::audio::{AudioListener, FreqData, get_note_from_frequency};

enum AppScreen {
    Debug,
    Tutor,
    Help,
}

pub enum TerminalMessage {
    Quit,
}

pub struct App {
    freq_data: FreqData,
    screen: AppScreen,
    input_file_path: Option<PathBuf>,
    tutor: Option<Tutor>,
}
impl App {
    pub fn new(input_file_path: Option<String>) -> Result<Self> {
        let input_file_path = input_file_path.map(PathBuf::from);
        let tutor = if let Some(input_file_path) = &input_file_path {
            Some(Self::set_tutor(input_file_path)?)
        } else {
            None
        };
        Ok(Self {
            freq_data: FreqData {
                data: vec![],
                max_magnitude: 0.0,
                peak_frequency: 0,
                fundamental_frequency: 0.0,
                samples_n: 0,
                sample_rate: 0,
                time_domain_samples: vec![],
            },
            screen: if input_file_path.is_some() {
                AppScreen::Tutor
            } else {
                AppScreen::Debug
            },
            input_file_path,
            tutor,
        })
    }

    pub fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
        let tick_rate = Duration::from_millis(250);
        let mut last_tick = Instant::now();
        let (tx, rx) = mpsc::channel();
        let (tx_to_audio, rx_from_ui) = mpsc::channel();
        let audio_thread = std::thread::spawn(move || {
            AudioListener::new(tx, rx_from_ui).run().unwrap();
        });
        loop {
            terminal.draw(|frame| self.draw(frame))?;

            let timeout = tick_rate.saturating_sub(last_tick.elapsed());
            if event::poll(timeout)? {
                if let Event::Key(key) = event::read()? {
                    if key.code == KeyCode::Char('q') {
                        tx_to_audio.send(TerminalMessage::Quit).unwrap();
                        break;
                    } else if key.code == KeyCode::Char('d') {
                        self.set_screen(AppScreen::Debug)?;
                    } else if key.code == KeyCode::Char('t') {
                        self.set_screen(AppScreen::Tutor)?;
                    } else if key.code == KeyCode::Char('h') {
                        self.set_screen(AppScreen::Help)?;
                    }
                }
            }
            if last_tick.elapsed() >= tick_rate {
                if let Ok(data) = rx.try_recv() {
                    let mut latest_data = data;
                    while let Ok(data) = rx.try_recv() {
                        latest_data = data;
                    }
                    self.on_tick(latest_data);
                }
                last_tick = Instant::now();
            }
        }
        audio_thread.join().unwrap();
        Ok(())
    }
    fn on_tick(&mut self, data: FreqData) {
        self.freq_data = data;
        if self.freq_data.max_magnitude > 10.0 {
            if let Some(note) = get_note_from_frequency(self.freq_data.fundamental_frequency) {
                if let Some(tutor) = self.tutor.as_mut() {
                    if let Some(next_note) = tutor.notes_sequence.get(tutor.current_note_index) {
                        if let Ok(current_note) = note.parse::<MusicalNote>() {
                            if let MusicalSound::Note(next_note) = next_note {
                                if current_note == *next_note
                                    && tutor.current_note_index < tutor.notes_sequence.len()
                                {
                                    let mut next_idx = tutor.current_note_index + 1;
                                    if next_idx >= tutor.notes_sequence.len() {
                                        tutor.current_note_index = next_idx;
                                    } else {
                                        while next_idx < tutor.notes_sequence.len()
                                            && matches!(
                                                tutor.notes_sequence[next_idx],
                                                MusicalSound::Silence
                                            )
                                        {
                                            next_idx += 1;
                                        }
                                        tutor.current_note_index = next_idx;
                                    }
                                }
                            }
                        }
                    }
                }
            };
        }
    }
    fn set_screen(&mut self, screen: AppScreen) -> Result<()> {
        if let AppScreen::Tutor = screen {
            self.reset_tutor()?;
        }
        self.screen = screen;
        Ok(())
    }
    fn reset_tutor(&mut self) -> Result<()> {
        if let Some(input_file_path) = &self.input_file_path {
            self.tutor = Some(Self::set_tutor(input_file_path)?);
        } else {
            self.tutor = None;
        }
        Ok(())
    }
    fn set_tutor(input_file_path: &Path) -> Result<Tutor> {
        // if let Some(input_file) = &input_file_path {
        let file_content = std::fs::read_to_string(input_file_path)?;
        let musical_sounds = Self::parse_musical_sounds(file_content)?;
        Ok(Tutor::new(musical_sounds))
        // } else {
        //     None
        // }
    }
    fn parse_musical_sounds(file_content: String) -> Result<Vec<MusicalSound>> {
        let lines = file_content
            .lines()
            .map(|line| {
                line.split(",")
                    .map(|n| n.parse::<MusicalNote>().map(MusicalSound::Note))
                    .collect::<Vec<_>>()
                    .into_iter()
                    .collect::<Result<Vec<_>, Error>>()
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let sounds = itertools::intersperse(lines, vec![MusicalSound::Silence])
            .flatten()
            .collect::<Vec<_>>();
        Ok(sounds)
    }
    fn draw(&self, frame: &mut Frame) {
        match self.screen {
            AppScreen::Tutor => {
                let layout = frame.area();
                if let Some(tutor) = &self.tutor {
                    let note = get_note_from_frequency(self.freq_data.fundamental_frequency);
                    let mut lines = vec![];
                    lines.push(
                        Line::from(format!(
                            "Current note: {}",
                            if let Some(note) = note {
                                note.to_string()
                            } else {
                                "Unknown".to_string()
                            }
                        ))
                        .centered(),
                    );
                    let mut spans = vec![];
                    for (i, sound) in tutor.notes_sequence.iter().enumerate() {
                        match sound {
                            MusicalSound::Note(n) => {
                                let content = n.to_string();
                                let span = Span::styled(
                                    content,
                                    if tutor.current_note_index == i {
                                        Style::default().add_modifier(Modifier::BOLD)
                                    } else if tutor.current_note_index < i {
                                        Style::default().fg(Color::Gray)
                                    } else {
                                        Style::default()
                                    },
                                );
                                spans.push(span);
                            }
                            MusicalSound::Silence => {
                                lines.push(Line::from(spans).centered());
                                spans = vec![];
                            }
                        }
                    }
                    if !spans.is_empty() {
                        lines.push(Line::from(spans).centered());
                    }
                    if tutor.current_note_index >= tutor.notes_sequence.len() {
                        lines.push(Line::from(
                            "Congratulations!! You have completed this.. let's gooo",
                        ));
                    }
                    let text = Text::from(lines);

                    frame.render_widget(text, layout);
                } else {
                    let layout = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                        .split(frame.area());
                    self.show_help(frame, layout[0]);
                    frame.render_widget(
                        Line::from("You need to pass a file as an argument to see the notes here."),
                        layout[1],
                    );
                }
            }
            AppScreen::Debug => {
                let layout = Layout::default()
                    .direction(ratatui::layout::Direction::Vertical)
                    .constraints([
                        Constraint::Length(5),
                        Constraint::Ratio(1, 2),
                        Constraint::Ratio(1, 2),
                    ])
                    .split(frame.area());
                let top = layout[0];
                let middle = layout[1];
                let bottom = layout[2];
                let note = get_note_from_frequency(self.freq_data.fundamental_frequency);
                let peak_freq_text = format!("Peak frequency: {}", self.freq_data.peak_frequency);
                let max_magnitude_text = format!("Max Magnitude: {}", self.freq_data.max_magnitude);
                let text_left = Text::from(vec![
                    Line::from(if let Some(note) = note {
                        format!("Note: {note}")
                    } else {
                        "Note: unknown".to_string()
                    })
                    .centered(),
                    Line::from(peak_freq_text),
                    Line::from(format!(
                        "Fundamental frequency (HPS): {}",
                        self.freq_data.fundamental_frequency
                    )),
                ])
                .centered();
                let top_layout = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .split(top);
                let text_right = Text::from(vec![
                    Line::from(format!("Sample rate: {}", self.freq_data.sample_rate)),
                    Line::from(max_magnitude_text),
                ]);
                frame.render_widget(
                    Paragraph::new(text_left).block(Block::bordered()),
                    top_layout[0],
                );
                frame.render_widget(
                    Paragraph::new(text_right).block(Block::bordered()),
                    top_layout[1],
                );
                self.render_freqs(frame, middle);
                self.render_time_domain(frame, bottom);
            }
            AppScreen::Help => {
                self.show_help(frame, frame.area());
            }
        }
    }

    fn show_help(&self, frame: &mut Frame, area: Rect) {
        let mut lines = vec![];
        lines.push(Line::from("h: help"));
        lines.push(Line::from("d: debug and visualization"));
        lines.push(Line::from("t: tutor"));
        lines.push(Line::from("q: quit"));
        let text = Text::from(lines).centered();
        frame.render_widget(text, area);
    }

    fn render_time_domain(&self, frame: &mut Frame, area: Rect) {
        if self.freq_data.time_domain_samples.is_empty() {
            return;
        }
        let data = self.freq_data.time_domain_samples.clone();
        let x_bounds = (0, data.len());
        let x_labels = vec![
            Span::styled(
                format!("{:.2}", x_bounds.0),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:.2}", x_bounds.1),
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ];
        let data = data
            .iter()
            .enumerate()
            .map(|(i, d)| (i as f64, *d as f64 * 1000.0))
            .collect::<Vec<_>>();
        let datasets = vec![
            Dataset::default()
                .marker(symbols::Marker::Braille)
                .style(Style::default().fg(Color::Cyan))
                .graph_type(ratatui::widgets::GraphType::Line)
                .data(&data),
        ];
        let y_bounds = (-50.0, 50.0);

        let chart = Chart::new(datasets)
            .block(
                Block::bordered()
                    .title("Time domain")
                    .title_alignment(ratatui::layout::Alignment::Center),
            )
            .x_axis(
                Axis::default()
                    .title("Time".to_string())
                    .style(Style::default().fg(Color::Gray))
                    .labels(x_labels)
                    .bounds([x_bounds.0 as f64, x_bounds.1 as f64]),
            )
            .y_axis(
                Axis::default()
                    .title("Magnitude")
                    .style(Style::default().fg(Color::Gray))
                    .labels(vec![
                        Span::styled(format!("{}", y_bounds.0), Style::default()),
                        Span::styled(format!("{}", y_bounds.1), Style::default()),
                    ])
                    .bounds([y_bounds.0, y_bounds.1]),
            );

        frame.render_widget(chart, area);
    }
    fn render_freqs(&self, frame: &mut Frame, area: Rect) {
        if self.freq_data.data.is_empty() {
            return;
        }
        // let n = self.freq_data.data.len() / 4;
        // let x_bounds = (self.freq_data.data[0].0, self.freq_data.data[n].0);
        let n = 1500.0;
        let x_bounds = (self.freq_data.data[0].0, n);
        let x_labels = vec![
            Span::styled(
                format!("{:.2}", x_bounds.0),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:.2}", x_bounds.1),
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ];
        let total_points = (n * self.freq_data.samples_n as f64 / self.freq_data.sample_rate as f64)
            .floor() as usize
            + 1;
        let n_chunks = 4;
        let chunk_size = total_points / n_chunks;
        let chunks = self.freq_data.data.chunks(chunk_size);
        let datasets = chunks
            .enumerate()
            .map(|(i, c)| {
                let multiple = 255 / (n_chunks + 1);
                Dataset::default()
                    .name(format!("freq{i}"))
                    .marker(symbols::Marker::Braille)
                    .style(Style::default().fg(
                        // if i % 2 == 0 {
                        //     Color::Cyan
                        // } else {
                        //     Color::Yellow
                        // },
                        if i < n_chunks + 1 {
                            Color::Rgb(
                                255 - i as u8 * multiple as u8,
                                i as u8 * multiple as u8,
                                255,
                            )
                        } else {
                            Color::Cyan
                        },
                    ))
                    .graph_type(ratatui::widgets::GraphType::Line)
                    .data(c)
            })
            .collect::<Vec<_>>();

        let chart = Chart::new(datasets)
            .block(
                Block::bordered()
                    .title("Frequencies")
                    .title_alignment(ratatui::layout::Alignment::Center),
            )
            .x_axis(
                Axis::default()
                    .title("Frequency".to_string())
                    .style(Style::default().fg(Color::Gray))
                    .labels(x_labels)
                    .bounds([x_bounds.0, x_bounds.1]),
            )
            .y_axis(
                Axis::default()
                    .title("Magnitude")
                    .style(Style::default().fg(Color::Gray))
                    .labels(vec![
                        Span::styled("0", Style::default()),
                        Span::styled("40", Style::default()),
                    ])
                    .bounds([0.0, 40.0]),
            );

        frame.render_widget(chart, area);
    }
}

struct Tutor {
    notes_sequence: Vec<MusicalSound>,
    current_note_index: usize,
}

#[derive(Clone, Debug)]
enum MusicalSound {
    Silence,
    Note(MusicalNote),
}

#[derive(Clone, Debug, PartialEq)]
enum MusicalNote {
    A,
    ASharp,
    B,
    BSharp,
    C,
    CSharp,
    D,
    DSharp,
    E,
    ESharp,
    F,
    FSharp,
    G,
    GSharp,
}

impl FromStr for MusicalNote {
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(match s {
            "A" => MusicalNote::A,
            "A#" => MusicalNote::ASharp,
            "B" => MusicalNote::B,
            "B#" => MusicalNote::BSharp,
            "C" => MusicalNote::C,
            "C#" => MusicalNote::CSharp,
            "D" => MusicalNote::D,
            "D#" => MusicalNote::DSharp,
            "E" => MusicalNote::E,
            "E#" => MusicalNote::ESharp,
            "F" => MusicalNote::F,
            "F#" => MusicalNote::FSharp,
            "G" => MusicalNote::G,
            "G#" => MusicalNote::GSharp,
            _ => return Err(Error::msg("couldn't parse")),
        })
    }
}

impl Display for MusicalNote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                MusicalNote::A => "A",
                MusicalNote::ASharp => "A#",
                MusicalNote::B => "B",
                MusicalNote::BSharp => "B#",
                MusicalNote::C => "C",
                MusicalNote::CSharp => "C#",
                MusicalNote::D => "D",
                MusicalNote::DSharp => "D#",
                MusicalNote::E => "E",
                MusicalNote::ESharp => "E#",
                MusicalNote::F => "F",
                MusicalNote::FSharp => "F#",
                MusicalNote::G => "G",
                MusicalNote::GSharp => "G#",
            },
        )
    }
}

impl Tutor {
    fn new(notes: Vec<MusicalSound>) -> Self {
        Self {
            notes_sequence: notes,
            current_note_index: 0,
        }
    }
}
