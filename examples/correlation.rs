use std::f32::consts::PI;

fn main() {
    const FS: usize = 48000;
    const T: usize = 1;
    let f0 = 196.0;
    let f1 = f0 * 2.0;
    let f2 = f0 * 3.0;

    let mut data = [0.0f32; FS * T];

    for (i, d) in data.iter_mut().enumerate() {
        *d = (2.0 * PI * f0 * i as f32 / FS as f32).sin()
            + (2.0 * PI * f1 * i as f32 / FS as f32).sin()
            + (2.0 * PI * f2 * i as f32 / FS as f32).sin();
    }

    let expected = FS as f32 / f0;
    println!("expected {}", expected);

    println!("data {:?}", &data[0..(expected * 0.1) as usize]);
    println!(
        "data+offset {:?}",
        &data[expected as usize..(expected * 1.1) as usize]
    );

    let mut curr_delay = expected as usize - 20;
    let window = curr_delay / 4;
    loop {
        println!("curr_delay {}", curr_delay);
        let low = convolution_sum(window, curr_delay - 1, &data);
        let mid = convolution_sum(window, curr_delay, &data);
        let high = convolution_sum(window, curr_delay + 1, &data);

        if mid < low && mid < high {
            break;
        } else if low < mid {
            curr_delay -= 1;
        } else if high < mid {
            curr_delay += 1;
        } else {
            panic!()
        }
    }
    println!(
        "curr_delay {}, curr_freq {}",
        curr_delay,
        FS as f32 / curr_delay as f32
    );
}

fn convolution_sum(size: usize, delay: usize, data: &[f32]) -> f32 {
    // let mut sum = 0.0;
    // let mut diff = 0.0;
    let mut diff_square = 0.0;

    for i in 0..size {
        // sum += data[i] * data[i + delay];
        // diff += (data[i] - data[i + delay]).abs();
        diff_square += (data[i] - data[i + delay]).powi(2);
    }

    // println!("\ndelay {}", delay);
    // println!("sum {}", sum);
    // println!("diff {}", diff);
    println!("diff_square {}", diff_square);
    diff_square
}
