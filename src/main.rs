use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicI32, AtomicU16},
    },
    time::Duration,
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rustfft::{FftPlanner, num_complex::Complex};
const NOTES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let host = cpal::default_host();
    let input_device = host
        .default_input_device()
        .expect("No default input device found");
    // let output_device = host
    //     .default_output_device()
    //     .expect("No default output device found");
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
                        for i in 0..max_k {
                            let freq = i as u32 * sample_rate / n as u32;
                            let magnitude = buffer[i].norm();
                            if magnitude > max_magnitude {
                                max_magnitude = magnitude;
                                max_magnitude_freq = freq;
                            }
                        }
                        println!(
                            "Peak frequency is {max_magnitude_freq} with magnitude {max_magnitude}"
                        );
                        // if max_magnitude >= 7.0 {
                        let midi_note_number =
                            (12.0 * (max_magnitude_freq as f32 / 440.0).log2() + 69.0).round();
                        let note = note_from_midi_note_number(midi_note_number as i32);
                        println!(
                            "m{midi_note_number} n{note} d{}",
                            midi_note_number / 12.0 + 1.0
                        );
                        // }

                        samples.clear();
                    }
                    // sum_of_squares.fetch_add(
                    //     (left_sample * 10000.0 * left_sample).floor() as i32,
                    //     Ordering::Relaxed,
                    // );
                    // n_samples.fetch_add(1, Ordering::Relaxed);
                    // if n_samples
                    //     .compare_exchange(rms_window_size, 0, Ordering::Relaxed, Ordering::Relaxed)
                    //     .is_ok()
                    // {
                    //     let sum = sum_of_squares.swap(0, Ordering::Relaxed);
                    //     let volume = 10.0 * (sum as f32 / rms_window_size as f32).sqrt();
                    //     println!("Volume: {volume}");
                    // }
                    // println!("Left channel sample: {}", left_sample);
                    // if channels > 1 {
                    //     let right_sample = sample[1];
                    //     println!("Right channel sample: {}", right_sample);
                    // }
                }
                // do stuff
            },
            move |err| {
                println!("Error: {err}");
            },
            None,
        )
        .expect("uhhhh error in stream input building");
    stream.play()?;
    println!("ok. started playing");
    std::thread::sleep(Duration::from_secs(10));
    println!("Sample rate: {sample_rate}");
    println!("Channels: {channels}");
    println!("end");
    Ok(())
}

fn note_from_midi_note_number(midi_note_number: i32) -> String {
    let i = midi_note_number % 12;
    format!("{}", NOTES[i as usize])
}
