use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicI32, AtomicU16},
        mpsc::{self, Receiver, Sender},
    },
    time::{Duration, Instant},
};

use color_eyre::eyre::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ratatui::{
    DefaultTerminal, Frame,
    crossterm::{
        event::{self, Event, KeyCode},
        style::Stylize,
    },
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::Span,
    widgets::{Axis, Block, Chart, Dataset},
};
use rustfft::{FftPlanner, num_complex::Complex};
const NOTES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];
fn main() -> Result<()> {
    color_eyre::install()?;
    let terminal = ratatui::init();
    let app_result = App::new().run(terminal);
    ratatui::restore();
    app_result
}

struct App {
    freq_data: FreqData,
}
impl App {
    fn new() -> Self {
        Self { freq_data: vec![] }
    }

    fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
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
        let [top, bottom] = Layout::vertical([Constraint::Fill(1); 2]).areas(frame.area());
        let [animated_chart, bar_chart] =
            Layout::horizontal([Constraint::Fill(1), Constraint::Length(29)]).areas(top);
        let [line_chart, scatter] = Layout::horizontal([Constraint::Fill(1); 2]).areas(bottom);

        self.render_freqs(frame, animated_chart);
    }

    fn render_freqs(&self, frame: &mut Frame, area: Rect) {
        if self.freq_data.len() == 0 {
            return;
        }
        let mid_freq = self.freq_data[self.freq_data.len() / 2];
        let x_labels = vec![
            Span::styled(format!("x1"), Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!("x2,{}", mid_freq.1)),
            Span::styled(format!("x3"), Style::default().add_modifier(Modifier::BOLD)),
        ];
        let datasets = vec![
            Dataset::default()
                .name("data2")
                .marker(symbols::Marker::Braille)
                .style(Style::default().fg(Color::Cyan))
                .graph_type(ratatui::widgets::GraphType::Line)
                .data(&self.freq_data),
            // Dataset::default()
            //     .name("data3")
            //     .marker(symbols::Marker::Braille)
            //     .style(Style::default().fg(Color::Yellow))
            //     .data(&self.freq_data),
        ];

        let chart = Chart::new(datasets)
            .block(Block::bordered())
            .x_axis(
                Axis::default()
                    .title("X Axis")
                    .style(Style::default().fg(Color::Gray))
                    .labels(x_labels)
                    .bounds([self.freq_data[0].0, self.freq_data.last().unwrap().0]),
            )
            .y_axis(
                Axis::default()
                    .title("Y Axis")
                    .style(Style::default().fg(Color::Gray))
                    // .labels(["-20".into(), "0".into(), "20".into()])
                    .bounds([0.0, 40.0]),
            );

        frame.render_widget(chart, area);
    }
}

enum TerminalMessage {
    Quit,
}

type FreqData = Vec<(f64, f64)>;

struct AudioListener {
    freq_dump_channel: Sender<FreqData>,
    terminal_msg_receiver: Receiver<TerminalMessage>,
}

impl AudioListener {
    fn new(
        freq_dump_channel: Sender<FreqData>,
        terminal_msg_receiver: Receiver<TerminalMessage>,
    ) -> Self {
        Self {
            freq_dump_channel,
            terminal_msg_receiver,
        }
    }

    fn run(&self) -> Result<()> {
        let host = cpal::default_host();
        let input_device = host
            .default_input_device()
            .expect("No default input device found");
        let mut supported_input_configs_range = input_device
            .supported_input_configs()
            .expect("Error querying supported configs");
        let supported_input_config = supported_input_configs_range
            .next()
            .expect("No supported config")
            .with_max_sample_rate();
        let config = supported_input_config.config();
        let sample_rate = config.sample_rate.0;
        let channels = config.channels as usize;
        let _sum_of_squares = AtomicI32::new(0);
        let rms_window_size = 1024;
        let _n_samples = AtomicU16::new(0);
        let samples: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(vec![]));
        let freq_dump_channel = self.freq_dump_channel.clone();
        let stream = input_device
            .build_input_stream(
                &config,
                move |data: &[f32], _| {
                    let mut samples = samples.lock().unwrap();
                    for sample in data.chunks_exact(channels) {
                        let left_sample = sample[0];
                        samples.push(left_sample);
                        if samples.len() >= rms_window_size {
                            let n = samples.len();
                            let mut planner = FftPlanner::new();
                            let fft = planner.plan_fft_forward(n);
                            let mut buffer = samples
                                .iter()
                                .map(|sample| Complex {
                                    re: *sample,
                                    im: 0.0,
                                })
                                .collect::<Vec<_>>();
                            fft.process(&mut buffer);

                            // max freq
                            let max_k = n / 2 + 1;
                            let mut max_magnitude_freq = 0;
                            let mut max_magnitude = buffer[0].norm();
                            let mut freq_data = vec![];
                            for i in 0..max_k {
                                let freq = i as u32 * sample_rate / n as u32;
                                let magnitude = buffer[i].norm();
                                freq_data.push((freq as f64, magnitude as f64));
                                if magnitude > max_magnitude {
                                    max_magnitude = magnitude;
                                    max_magnitude_freq = freq;
                                }
                            }
                            freq_dump_channel.send(freq_data).unwrap();
                            // println!(
                            //     "Peak frequency is {max_magnitude_freq} with magnitude {max_magnitude}"
                            // );
                            let midi_note_number =
                                (12.0 * (max_magnitude_freq as f32 / 440.0).log2() + 69.0).round();
                            if midi_note_number.is_finite() {
                                let note = note_from_midi_note_number(midi_note_number as i32);
                                // println!(
                                //     "m{midi_note_number} n{note} d{}",
                                //     midi_note_number / 12.0 + 1.0
                                // );
                            }

                            samples.clear();
                        }
                    }
                },
                move |err| {
                    // println!("Error: {err}");
                },
                None,
            )
            .expect("uhhhh error in stream input building");
        stream.play()?;
        // println!("ok. started playing");
        loop {
            match self.terminal_msg_receiver.try_recv() {
                Ok(TerminalMessage::Quit) => {
                    // println!("Quit signal recieved");
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    // ignore
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    // println!("UI thread disconnected.. kinda weird tbh");
                    break;
                }
            }
        }
        // println!("Sample rate: {sample_rate}");
        // println!("Channels: {channels}");
        // println!("end");
        Ok(())
    }
}

fn note_from_midi_note_number(midi_note_number: i32) -> String {
    let i = midi_note_number % 12;
    format!("{}", NOTES[i as usize])
}
