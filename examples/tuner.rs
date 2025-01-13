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

    // #[derive(PartialEq)]
    // enum Sign {
    //     Neg,
    //     Pos,
    // }

    // let mut sample_count = 0;
    // let mut sign = Sign::Neg;
    // let mut freq = 0.0;

    let fs = config.sample_rate.0;
    println!("sample rate {}", fs);

    // let f0 = 109.hz();
    // let fs = 48.khz();
    // // Create coefficients for the biquads
    // let coeffs =
    //     Coefficients::<f32>::from_params(Type::BandPass, fs, f0, Q_BUTTERWORTH_F32).unwrap();

    // // Create two different biquads
    // let mut biquad1 = DirectForm1::<f32>::new(coeffs);
    // let mut biquad2 = DirectForm2Transposed::<f32>::new(coeffs);

    // const BUF_SIZE: usize = 32;
    // let mut index: usize = 0;
    // let mut buf = [0.0; BUF_SIZE];
    // let mut freq_average  = 0.0;

    const HISTORY_SIZE: usize = 2048; // we need
    let mut history = [0.0f32; HISTORY_SIZE];
    let mut index_h: usize = 0;
    let window = 100;

    fn convolution_sum(window: usize, delay: usize, index: usize, data: &[f32]) -> f32 {
        let mut diff_square = 0.0;

        for i in 0..window {
            diff_square += (data[(HISTORY_SIZE + index - i) % HISTORY_SIZE]
                - data[(HISTORY_SIZE + index - (i + delay)) % HISTORY_SIZE])
                .powi(2);
        }

        // println!("diff_square {}", diff_square);
        diff_square
    }

    // Low-E: 82 Hz (E2)
    // A: 110 Hz (A2)
    // D: 147 Hz (D3)
    // G: 196 Hz (G3)
    // B: 247 Hz (B3)
    // High-E: 330 Hz (E4)

    let fundamental_freq: [Hertz<f32>; 6] =
        [82.hz(), 110.hz(), 147.hz(), 196.hz(), 247.hz(), 330.hz()];
    let mut biquads: Vec<(DirectForm1<f32>, f32)> = fundamental_freq
        .iter()
        .map(|f0| {
            let coeffs: Coefficients<f32> =
                Coefficients::<f32>::from_params(Type::BandPass, fs.hz(), *f0, Q_BUTTERWORTH_F32)
                    .unwrap();
            (DirectForm1::<f32>::new(coeffs), 0.0)
        })
        .collect();

    // let f: f32 = 82.0; // E2
    // let f: f32 = 110.0; // A2
    // let f: f32 = 147.0; // D3
    // let f: f32 = 196.0; // G3
    // let f: f32 = 247.0; // B3
    let f: f32 = 330.0; // E4

    let initial_delay = (fs as f32 / f) as usize;
    let mut delay = initial_delay; // initial guess around G;
    println!("initial delay {}, freq {}", delay, fs as f32 / delay as f32);

    let mut count = 0;

    let input_data_fn = move |data: &[f32], _: &cpal::InputCallbackInfo| {
        let mut output_fell_behind = false;
        for &s in data {
            for (q, e) in biquads.iter_mut() {
                let sf = q.run(s);
                *e = (*e + sf * sf) / 2.0;
            }

            index_h = (index_h + 1) % HISTORY_SIZE;
            history[index_h] = s;

            if producer.try_push(s).is_err() {
                output_fell_behind = true;
            }
        }
        if output_fell_behind {
            eprintln!("output stream fell behind: try increasing latency");
        }

        count = (count + 1) % 100;
        if count == 0 {
            let es: Vec<_> = biquads.iter().map(|(_, e)| e).collect();
            let r: Vec<_> = es.iter().map(|e| e.sqrt()).collect();

            let index_of_max: Option<usize> = r
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.total_cmp(b))
                .map(|(index, _)| index);

            println!("max {:?}, es {:.2?}, r {:.2?}", index_of_max, es, r);
            // let mid = convolution_sum(window, delay, index_h, &history);
            // let mut low_delay = delay - 1;
            // let mut low = convolution_sum(window, low_delay, index_h, &history);
            // // let low_low_delay = delay - 10; //  (delay as f32 * 0.9) as usize;
            // // let low_low = convolution_sum(window, low_low_delay, index_h, &history);
            // let mut high_delay = delay + 1;
            // let mut high = convolution_sum(window, high_delay, index_h, &history);
            // // let high_high_delay = delay + 10; // (delay as f32 * 1.1) as usize;
            // // let high_high = convolution_sum(window, high_high_delay, index_h, &history);

            // // if low_low < low {
            // //     println!("low low");
            // //     low = low_low;
            // //     low_delay = low_low_delay;
            // // }

            // // if high_high > high {
            // //     println!("high high");
            // //     high = high_high;
            // //     high_delay = high_high_delay;
            // // }

            // println!("i {}", index_h);
            // if mid < low && mid < high {
            //     println!("ok");
            // } else if low < mid {
            //     println!("low");
            //     delay = low_delay;
            // } else if high < mid {
            //     println!("high");
            //     delay = high_delay;
            // } else {
            //     println!(
            //         "------------------------            low {:.02?}, mid {:.02?}, high {:.02?}",
            //         low, mid, high
            //     );
            //     panic!();
            // }

            // // clamp within reasonable bounds
            // delay = delay
            //     .min((initial_delay as f32 * 1.1) as usize)
            //     .max((initial_delay as f32 * 0.9) as usize);
            // println!("delay {}, freq {}", delay, fs as f32 / delay as f32);
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
