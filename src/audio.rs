use std::sync::{
    Arc, Mutex,
    mpsc::{self, Receiver, Sender},
};

use color_eyre::eyre::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rustfft::{FftPlanner, num_complex::Complex};

use crate::app::TerminalMessage;

pub struct FreqData {
    pub data: Vec<(f64, f64)>,
    pub peak_frequency: u32,
    pub fundamental_frequency: u32,
    pub max_magnitude: f64,
    pub sample_rate: u32,
    pub samples_n: usize,
    pub time_domain_samples: Vec<f32>,
}
// type FreqData = Vec<(f64, f64)>;

#[derive(Debug)]
pub struct AudioListener {
    freq_dump_channel: Sender<FreqData>,
    terminal_msg_receiver: Receiver<TerminalMessage>,
}

impl AudioListener {
    pub fn new(
        freq_dump_channel: Sender<FreqData>,
        terminal_msg_receiver: Receiver<TerminalMessage>,
    ) -> Self {
        Self {
            freq_dump_channel,
            terminal_msg_receiver,
        }
    }

    #[tracing::instrument]
    pub fn run(&self) -> Result<()> {
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
        let rms_window_size = 4096;
        let samples: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(vec![]));
        let freq_dump_channel = self.freq_dump_channel.clone();
        let epsilon = 1e-10;
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

                            let max_k = n / 2 + 1;
                            let mut downsampled_spectra = vec![];
                            let mut smallest_len = 1000000;
                            for i in 2..5 {
                                let downsampled_spectrum = (0..max_k)
                                    .step_by(i)
                                    .map(|j| buffer[j].norm().max(epsilon))
                                    .collect::<Vec<_>>();
                                let spec_len = downsampled_spectrum.len();
                                downsampled_spectra.push(downsampled_spectrum);
                                if spec_len < smallest_len {
                                    smallest_len = spec_len;
                                }
                            }
                            let mut log_product_spectrum = buffer[0..smallest_len]
                                .iter()
                                .map(|b| {
                                    let m = b.norm();
                                    20.0 * m.max(epsilon).log10()
                                })
                                .collect::<Vec<_>>();
                            let mut max_product_spectrum_i = 0;
                            let mut max_product_spectrum = f32::NEG_INFINITY;
                            for i in 0..smallest_len {
                                let mut log_psi = log_product_spectrum[i];
                                for spectrum in downsampled_spectra.iter() {
                                    log_psi += spectrum[i];
                                }
                                log_product_spectrum[i] = log_psi;
                                if log_psi > max_product_spectrum {
                                    max_product_spectrum_i = i;
                                    max_product_spectrum = log_psi;
                                }
                            }
                            let fundamental_frequency =
                                max_product_spectrum_i as u32 * sample_rate / n as u32;

                            let mut max_magnitude_freq = 0;
                            let mut max_magnitude = buffer[0].norm();
                            let mut freq_data = vec![];
                            for i in 0..max_k {
                                let freq = i as u32 * sample_rate / n as u32;
                                let magnitude = buffer[i].norm();
                                if freq <= 1500 {
                                    freq_data.push((freq as f64, magnitude as f64));
                                }
                                if magnitude > max_magnitude {
                                    max_magnitude = magnitude;
                                    max_magnitude_freq = freq;
                                }
                            }
                            freq_dump_channel
                                .send(FreqData {
                                    data: freq_data,
                                    max_magnitude: max_magnitude as f64,
                                    peak_frequency: max_magnitude_freq,
                                    fundamental_frequency,
                                    samples_n: n,
                                    sample_rate,
                                    time_domain_samples: samples.clone(),
                                })
                                .unwrap();
                            // println!(
                            //     "Peak frequency is {max_magnitude_freq} with magnitude {max_magnitude}"
                            // );

                            samples.clear();
                        }
                    }
                },
                move |err| {
                    println!("Error: {err}");
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
pub fn get_note_from_frequency(freq: u32) -> Option<String> {
    let midi_note_number = (12.0 * (freq as f32 / 440.0).log2() + 69.0).round();
    midi_note_number
        .is_finite()
        .then(|| note_from_midi_note_number(midi_note_number as i32))
}
const NOTES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];
