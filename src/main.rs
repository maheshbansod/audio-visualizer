use std::{
    sync::atomic::{AtomicI32, AtomicU16, Ordering},
    time::Duration,
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
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
    let sum_of_squares = AtomicI32::new(0);
    let rms_window_size = 1024;
    let n_samples = AtomicU16::new(0);
    let stream = input_device
        .build_input_stream(
            &config,
            move |data: &[f32], _| {
                println!("dlen {}", data.len());
                for sample in data.chunks_exact(channels) {
                    let left_sample = sample[0];
                    sum_of_squares.fetch_add(
                        (left_sample * 10000.0 * left_sample).floor() as i32,
                        Ordering::Relaxed,
                    );
                    n_samples.fetch_add(1, Ordering::Relaxed);
                    if n_samples
                        .compare_exchange(rms_window_size, 0, Ordering::Relaxed, Ordering::Relaxed)
                        .is_ok()
                    {
                        let sum = sum_of_squares.swap(0, Ordering::Relaxed);
                        let volume = 10.0 * (sum as f32 / rms_window_size as f32).sqrt();
                        println!("Volume: {volume}");
                    }
                    // println!("Left channel sample: {}", left_sample);
                    if channels > 1 {
                        let right_sample = sample[1];
                        println!("Right channel sample: {}", right_sample);
                    }
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
