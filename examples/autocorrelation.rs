use std::f32::consts::PI;

fn main() {
    const FS: usize = 48000;
    const T: usize = 1;
    let f0 = 330.0;
    // let f0 = 82.0;
    // let f0 = 147.0;
    // let f0 = 196.0;
    let f1 = f0 * 2.01; // 2.01;
    let f2 = f0 * 2.96; // 3.04;

    let mut data = [0.0f32; FS * T];

    for (i, d) in data.iter_mut().enumerate() {
        *d = 0.25
            * (1.0 * (2.0 * PI * f0 * i as f32 / FS as f32).sin()
                + 1.2 * (2.0 * PI * f1 * i as f32 / FS as f32).sin()
                + 1.1 * (2.0 * PI * f2 * i as f32 / FS as f32).sin());
    }

    let initial = 400.0;
    let expected = FS as f32 / initial;
    println!("expected {}", expected);

    let mut delay = expected.round() as usize;
    let offset = data.len() - 1; // end of buffer

    // search
    let mut best_corr = 0.0;
    let mut best_delay = 0;
    loop {
        let acc = autocorr(delay, offset, 0, &data);
        println!(
            "delay {}, freq {}, acc {}",
            delay,
            FS as f32 / delay as f32,
            acc,
        );

        if acc > best_corr {
            best_corr = acc;
            best_delay = delay;
            let acc2 = autocorr(delay, offset, delay, &data);
            println!("---- new best ---- acc2 {}", acc2);

            if (acc2 - acc).abs() < 0.05 * acc {
                break;
            }
        }

        delay += 1;

        if delay == 600 {
            panic!()
        }
    }

    let f = FS as f32 / best_delay as f32;
    println!("detected frequency {}", f);
}

// search towards longer delays (lower frequencies)
fn autocorr(delay: usize, offset: usize, offset2: usize, data: &[f32]) -> f32 {
    let mut acc = 0.0;

    for i in 0..delay {
        acc += data[offset - i] * data[offset - (i + delay + offset2)];
    }

    acc
}

// if acc_mid < 0.0 {
//     println!("-- bad match --");
//     return (false, delay + 1, acc_high);
// }

// println!(
//     "mid {:.4?}, high {:.4?}, high_high {:.4?} ",
//     acc_mid, acc_high, acc_high_high
// );

// if acc_mid > acc_high.max(acc_high_high) {
//     println!("mid");
//     (true, delay, acc_mid)
// } else {
//     println!("high");
//     (false, delay + 1, acc_high)
// }

// // track
// loop {
//     let (is_mid, new_delay, autocorr) = track(window, offset, delay, &data);
//     println!(
//         "is_mid {}, new_delay {}, autocorr {:.4?}",
//         is_mid, new_delay, autocorr
//     );
//     if is_mid {
//         break;
//     } else {
//         delay = new_delay;
//         window = new_delay;
//     }
// }

// let f = FS as f32 / delay as f32;
// println!("detected frequency {}", f);

// // track signal and adjust offset
// fn track(window: usize, offset: usize, delay: usize, data: &[f32]) -> (bool, usize, f32) {
//     let mut acc_mid = 0.0;
//     let mut acc_low = 0.0;
//     let mut acc_high = 0.0;

//     for i in 0..window {
//         let s = data[offset - i];
//         acc_mid += s * data[offset - (i + delay)];
//         acc_low += s * data[offset - (i + delay - 1)];
//         acc_high += s * data[offset - (i + delay + 1)];
//     }
//     acc_mid = acc_mid / window as f32;
//     acc_low = acc_low / window as f32;
//     acc_high = acc_high / window as f32;

//     println!(
//         "mid {:.4?}, high {:.4?}, low {:.4?}",
//         acc_mid, acc_high, acc_low
//     );

//     if acc_mid > acc_low.max(acc_high) {
//         println!("mid");
//         (true, delay, acc_mid)
//     } else if acc_high > acc_low {
//         println!("high");
//         (false, delay + 1, acc_high)
//     } else {
//         println!("low");
//         (false, delay - 1, acc_low)
//     }
// }

// println!("data {:?}", &data[0..(expected * 0.1) as usize]);
// println!(
//     "data+offset {:?}",
//     &data[expected as usize..(expected * 1.1) as usize]
// );

// let mut curr_delay = expected as usize - 20;
// let window = curr_delay / 4;
// loop {
//     println!("curr_delay {}", curr_delay);
//     let low = convolution_sum(window, curr_delay - 1, &data);
//     let mid = convolution_sum(window, curr_delay, &data);
//     let high = convolution_sum(window, curr_delay + 1, &data);

//     if mid < low && mid < high {
//         break;
//     } else if low < mid {
//         curr_delay -= 1;
//     } else if high < mid {
//         curr_delay += 1;
//     } else {
//         panic!()
//     }
// }
// println!(
//     "curr_delay {}, curr_freq {}",
//     curr_delay,
//     FS as f32 / curr_delay as f32
// );

#[test]
fn test_max() {
    println!("{}", 1.max(2));
}
