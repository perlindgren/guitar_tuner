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
    
    let host = 
        cpal::host_from_id(cpal::available_hosts()
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

    #[derive(PartialEq)]
    enum Sign {
        Neg,
        Pos,
    }

    let mut sample_count = 0;
    let mut sign = Sign::Neg;
    // let mut freq = 0.0;

    println!("sample rate {}", config.sample_rate.0);

    let f0 =109.hz();
    let fs = 48.khz();
    // Create coefficients for the biquads
    let coeffs =
        Coefficients::<f32>::from_params(Type::BandPass, fs, f0, Q_BUTTERWORTH_F32).unwrap();

    // Create two different biquads
    let mut biquad1 = DirectForm1::<f32>::new(coeffs);
    let mut biquad2 = DirectForm2Transposed::<f32>::new(coeffs);

    const BUF_SIZE: usize = 32;
    let mut index: usize = 0;
    let mut buf = [0.0; BUF_SIZE];
    let mut freq_average  = 0.0;


    let mut update_freq = move |sample_count| {
        let freq = 1.0 / (sample_count as f32 / config.sample_rate.0 as f32);

        if freq > 400.0 {
            return
        }

        buf[index] = freq;
        index = (index + 1) % BUF_SIZE;
        let sum: f32 = buf.iter().sum();
        freq_average = sum / BUF_SIZE as f32;

        println!("f {:.1?}\t average {:.1?}", freq, freq_average);
    };

    // let mut history = [1020]
    let input_data_fn = move |data: &[f32], _: &cpal::InputCallbackInfo| {
        // println!("in {}", data.len());

        let mut output_fell_behind = false;
        for &s in data {
            let sample = s;
            let sample = biquad1.run(sample);
            let sample = biquad1.run(sample);
            let sample = biquad1.run(sample);
            // let sample = biquad2.run(sample);
            // let sample = biquad2.run(sample);
            sample_count += 1;

            if sample > 0.0 && sign == Sign::Neg {
                update_freq(sample_count);
                sample_count = 0;
                sign = Sign::Pos;
            } else if sample < 0.0 && sign == Sign::Pos {
                sign = Sign::Neg;
            } 
            if producer.try_push(s).is_err() {
                output_fell_behind = true;
            }
        }
        if output_fell_behind {
            eprintln!("output stream fell behind: try increasing latency");
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
