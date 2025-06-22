use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

use color_eyre::eyre::Result;
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
    Main,
    FreqChart,
}

pub enum TerminalMessage {
    Quit,
}

pub struct App {
    freq_data: FreqData,
    screen: AppScreen,
}
impl App {
    pub fn new() -> Self {
        Self {
            freq_data: FreqData {
                data: vec![],
                max_magnitude: 0.0,
                peak_frequency: 0,
                fundamental_frequency: 0,
                samples_n: 0,
                sample_rate: 0,
                time_domain_samples: vec![],
            },
            screen: AppScreen::Main,
        }
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
                        self.screen = AppScreen::FreqChart;
                    } else if key.code == KeyCode::Char('m') {
                        self.screen = AppScreen::Main
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
    }
    fn draw(&self, frame: &mut Frame) {
        match self.screen {
            AppScreen::FreqChart => {
                let area = frame.area();

                self.render_freqs(frame, area);
            }
            AppScreen::Main => {
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
        }
    }

    fn render_time_domain(&self, frame: &mut Frame, area: Rect) {
        if self.freq_data.time_domain_samples.len() == 0 {
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
                    .title(format!("Time"))
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
        if self.freq_data.data.len() == 0 {
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
                    .title(format!("Frequency"))
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
