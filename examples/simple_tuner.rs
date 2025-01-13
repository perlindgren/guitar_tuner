//! Feeds back the input stream directly into the output stream.
//!
//! Assumes that the input and output devices can use the same stream configuration and that they
//! support the f32 sample format.
//!
//! Uses a delay of `LATENCY_MS` milliseconds in case the default input and output streams are not
//! precisely synchronised.

use std::{char::EscapeUnicode, thread::available_parallelism};

use biquad::*;
use clap::Parser;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::{
    traits::{Consumer, Producer, Split},
    HeapRb,
};

#[derive(Parser, Debug)]
#[command(version, about = "CPAL feedback example", long_about = None)]
struct Opt {
    /// The input audio device to use
    #[arg(short, long, value_name = "IN", default_value_t = String::from("default"))]
    input_device: String,

    /// The output audio device to use
    #[arg(short, long, value_name = "OUT", default_value_t = String::from("default"))]
    output_device: String,

    /// Specify the delay between input and output
    #[arg(short, long, value_name = "DELAY_MS", default_value_t = 150.0)]
    latency: f32,

    /// Use the JACK host
    #[cfg(all(
        any(
            target_os = "linux",
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "netbsd"
        ),
        feature = "jack"
    ))]
    #[arg(short, long)]
    #[allow(dead_code)]
    jack: bool,
}

fn main() -> anyhow::Result<()> {
    let opt = Opt::parse();

    // Manually check for flags. Can be passed through cargo with -- e.g.
    // cargo run --release --example beep --features jack -- --jack

    let host = cpal::host_from_id(cpal::available_hosts()
            .into_iter()
            .find(|id| *id == cpal::HostId::Jack)
            .expect(
                "make sure --features jack is specified. only works on OSes where jack is available",
            )).expect("jack host unavailable")
    ;

    // Find devices.
    let input_device = if opt.input_device == "default" {
        host.default_input_device()
    } else {
        host.input_devices()?
            .find(|x| x.name().map(|y| y == opt.input_device).unwrap_or(false))
    }
    .expect("failed to find input device");

    let output_device = if opt.output_device == "default" {
        host.default_output_device()
    } else {
        host.output_devices()?
            .find(|x| x.name().map(|y| y == opt.output_device).unwrap_or(false))
    }
    .expect("failed to find output device");

    println!("Using input device: \"{}\"", input_device.name()?);
    println!("Using output device: \"{}\"", output_device.name()?);

    // We'll try and use the same configuration between streams to keep it simple.
    let mut config: cpal::StreamConfig = input_device.default_input_config()?.into();
    config.channels = 1;

    // Create a delay in case the input and output devices aren't synced.
    let latency_frames = (opt.latency / 1_000.0) * config.sample_rate.0 as f32;
    let latency_samples = latency_frames as usize * config.channels as usize;

    // The buffer to share samples
    let ring = HeapRb::<f32>::new(latency_samples * 2);
    let (mut producer, mut consumer) = ring.split();

    // Fill the samples with 0.0 equal to the length of the delay.
    for _ in 0..latency_samples {
        // The ring buffer has twice as much space as necessary to add latency here,
        // so this should never fail
        producer.try_push(0.0).unwrap();
    }

    let fs = config.sample_rate.0;
    println!("sample rate {}", fs);

    const HISTORY_SIZE: usize = 2048; // we need
    let mut history = [0.0f32; HISTORY_SIZE];
    let mut index_h: usize = 0;

    fn diff_sum(window: usize, delay: usize, index: usize, data: &[f32]) -> f32 {
        let mut diff_square = 0.0;

        for i in 0..window {
            diff_square += (data[(HISTORY_SIZE + index - i) % HISTORY_SIZE]
                - data[(HISTORY_SIZE + index - (i + delay)) % HISTORY_SIZE])
                .powi(2);
        }

        // println!("diff_square {}", diff_square);
        diff_square
    }

    fn energy(window: usize, delay: usize, index: usize, data: &[f32]) -> f32 {
        let mut energy = 0.0;

        for i in 0..window {
            energy += data[(HISTORY_SIZE + index - i) % HISTORY_SIZE].powi(2);
        }

        // println!("diff_square {}", diff_square);
        energy
    }

    fn autocorr(delay: usize, offset: usize, offset2: usize, data: &[f32]) -> f32 {
        let mut acc = 0.0;

        for i in 0..delay {
            acc += data[(HISTORY_SIZE + offset - i) % HISTORY_SIZE]
                * data[(HISTORY_SIZE + offset - (i + delay + offset2)) % HISTORY_SIZE];
        }

        acc
    }

    let f: f32 = 82.0; // E2

    let f: f32 = 110.0; // A2

    // let f: f32 = 147.0; // D3

    // let f: f32 = 196.0; // G3

    // let f: f32 = 247.0; // B3

    // let f: f32 = 330.0; // E4

    let f = 400.0; // initial guess

    let initial_delay = (fs as f32 / f) as usize;
    let mut current_delay = initial_delay;
    let mut delay = initial_delay; // initial guess around G;
    println!("initial delay {}, freq {}", delay, fs as f32 / delay as f32);

    let mut count = 0;

    #[derive(PartialEq, Debug)]
    enum Mode {
        Search,
        Track,
    }

    let mut mode = Mode::Search;
    let mut best_corr = 0.0;

    let mut energy_average = 0.0;
    let input_data_fn = move |data: &[f32], _: &cpal::InputCallbackInfo| {
        // println!("data.len {}", data.len());
        let mut output_fell_behind = false;
        for &s in data {
            index_h = (index_h + 1) % HISTORY_SIZE;
            history[index_h] = s;

            if producer.try_push(s).is_err() {
                output_fell_behind = true;
            }
        }
        if output_fell_behind {
            eprintln!("output stream fell behind: try increasing latency");
        }

        // count = (count + 1) % 100;
        // if count == 0 {

        match mode {
            Mode::Search => {
                println!("-- iterate search mode --");
                loop {
                    let acc = autocorr(delay, index_h, 0, &history);
                    // println!(
                    //     "delay {}, freq {}, acc {}",
                    //     delay,
                    //     fs as f32 / delay as f32,
                    //     acc,
                    // );

                    if acc > best_corr {
                        best_corr = acc;

                        let acc2 = autocorr(delay, index_h, delay, &history);
                        let error = (acc2 - acc).abs();
                        println!(
                            "---- new best ---- delay {} acc2 {}, err {}",
                            delay, acc2, error
                        );

                        if error < 0.0025 * acc {
                            println!(
                                "-- new frequency found -- delay {}, freq {}",
                                delay,
                                fs as f32 / delay as f32
                            );
                            current_delay = delay;
                            mode = Mode::Track;
                            break;
                        }
                    }

                    if delay < 600 {
                        delay += 1;
                    } else {
                        best_corr = 0.0;
                        delay = initial_delay;
                        break;
                    }
                }
            }
            Mode::Track => {
                let mid = diff_sum(delay, delay, index_h, &history);
                let low = diff_sum(delay, delay - 1, index_h, &history);
                let high = diff_sum(delay, delay + 1, index_h, &history);

                let energy = energy(delay, delay, index_h, &history);

                //  println!("energy = {}, average {}", energy, energy_average);
                if energy > 4.0 * energy_average {
                    println!("-----------------------------------------------------");
                    delay = initial_delay;
                    best_corr = 0.0;
                    mode = Mode::Search
                }
                energy_average = 0.5 * (energy_average + energy);

                if mid < low && mid < high {
                    //   println!("ok ");
                } else if low < mid {
                    //  println!("low");
                    delay -= 1;
                } else if high < mid {
                    //    println!("high");
                    delay += 1;
                } else {
                    // println!(
                    // "------------------------            low {:.02?}, mid {:.02?}, high {:.02?}",
                    // low, mid, high
                    // );
                    // panic!();
                }

                // clamp within reasonable bounds
                delay = delay
                    .min((current_delay as f32 * 1.2) as usize)
                    .max((current_delay as f32 * 0.8) as usize);
                println!("delay {}, freq {}", delay, fs as f32 / delay as f32);
            }
        }
    };

    let output_data_fn = move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
        // println!("out {}", data.len());
        let mut input_fell_behind = false;
        for sample in data {
            *sample = match consumer.try_pop() {
                Some(s) => s,
                None => {
                    input_fell_behind = true;
                    0.0
                }
            };
        }
        if input_fell_behind {
            eprintln!("input stream fell behind: try increasing latency");
        }
    };

    // Build streams.
    println!(
        "Attempting to build both streams with f32 samples and `{:?}`.",
        config
    );
    let input_stream = input_device.build_input_stream(&config, input_data_fn, err_fn, None)?;
    let output_stream = output_device.build_output_stream(&config, output_data_fn, err_fn, None)?;
    println!("Successfully built streams.");

    // Play the streams.
    println!(
        "Starting the input and output streams with `{}` milliseconds of latency.",
        opt.latency
    );
    input_stream.play()?;
    output_stream.play()?;

    // Run for 3 seconds before closing.
    loop {}
    // println!("Playing for 3 seconds... ");
    // std::thread::sleep(std::time::Duration::from_secs(3));
    // drop(input_stream);
    // drop(output_stream);
    // println!("Done!");
    Ok(())
}

fn err_fn(err: cpal::StreamError) {
    eprintln!("an error occurred on stream: {}", err);
}
