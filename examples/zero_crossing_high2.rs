use biquad::*;
use guitar_tuner::lerp::lerp_zero;
use std::fs::read_to_string;

fn main() {
    let file = "./nrf52840/rtic_app/octave/dhigh.txt";

    let s = read_lines(file);
    let coeffs =
        Coefficients::<f32>::from_params(Type::HighPass, 1.khz(), 300.hz(), Q_BUTTERWORTH_F32)
            .unwrap();

    // Create two different biquads
    let mut biquad1 = DirectForm1::<f32>::new(coeffs);

    let mut hp1 = Vec::new();
    let mut hp2 = Vec::new();
    let mut hp3 = Vec::new();

    // Run for all the inputs
    for elem in &s {
        hp1.push(biquad1.run(*elem));
    }

    // Run for all the inputs
    for elem in &hp1 {
        hp2.push(biquad1.run(*elem));
    }

    // Run for all the inputs
    for elem in &hp2 {
        hp3.push(biquad1.run(*elem));
    }

    let nr_cross = process(s);
    println!("no filter {}", nr_cross);
    let nr_cross = process(hp1);
    println!("hp1 {}", nr_cross);
    let nr_cross = process(hp2);
    println!("hp2 {}", nr_cross);
    let nr_cross = process(hp3);
    println!("hp3 {}", nr_cross);
}
fn read_lines(filename: &str) -> Vec<f32> {
    read_to_string(filename)
        .unwrap() // panic on possible file-reading errors
        .lines() // split the string into an iterator of string slices
        .map(|s| s.trim().parse::<i32>().unwrap_or(0) as f32) // make each slice into a string
        .collect() // gather them together into a vector
}

fn process(s: Vec<f32>) -> u32 {
    let mut prev = 0.0;
    let mut nr_zero = 0;
    for (i, y) in s.into_iter().enumerate() {
        if y > 0.0 && prev < 0.0 {
            nr_zero += 1;
            // let offset = lerp_zero(prev, y);
            // println!("i {} prev {} y {}, offset {}", i, prev, y, offset);
        }
        prev = y;
    }
    nr_zero
}
