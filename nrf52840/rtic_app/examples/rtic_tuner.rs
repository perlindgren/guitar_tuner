// examples/rtic_tuner.rs

#![no_main]
#![no_std]

use hal::pac;
use nrf52840_hal::{self as hal, pac::SAADC};
use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};

const BUFFER_SIZE: usize = 1024; // approximately 1second of data at 1kHz
type Buffer = [i16; BUFFER_SIZE];

#[rtic::app(device = pac, dispatchers = [UARTE1, UARTE0_UART0])]
mod app {
    use super::*;
    use cortex_m::asm;

    use fugit::ExtU32;
    use hal::monotonic::MonotonicTimer;

    use pac::{
        saadc::{ch::config::*, oversample::OVERSAMPLE_A, resolution::VAL_A},
        TIMER0,
    };

    const TIMER_HZ: u32 = 16_000_000; // 16 MHz

    #[monotonic(binds = TIMER0, default = true)]
    type MyMono = MonotonicTimer<TIMER0, TIMER_HZ>;

    #[shared]
    struct Shared {}

    #[local]
    struct Local {
        saadc: SAADC,
    }

    #[init]
    fn init(cx: init::Context) -> (Shared, Local, init::Monotonics) {
        rtt_init_print!();
        rprintln!("\n--- rtic tuner ---\n");

        let mono = MyMono::new(cx.device.TIMER0);
        let saadc = cx.device.SAADC;
        saadc.enable.write(|w| w.enable().enabled());
        saadc.resolution.write(|w| w.val().variant(VAL_A::_14BIT));
        saadc
            .oversample
            .write(|w| w.oversample().variant(OVERSAMPLE_A::BYPASS));
        saadc.samplerate.write(|w| w.mode().task());

        saadc.ch[0].config.write(|w| {
            w.refsel().variant(REFSEL_A::INTERNAL);
            w.gain().variant(GAIN_A::GAIN4);
            w.tacq().variant(TACQ_A::_20US);
            w.mode().variant(MODE_A::DIFF);
            w.resp().variant(RESP_A::BYPASS);
            w.resn().variant(RESN_A::BYPASS);
            w.burst().disabled();
            w
        });
        saadc.ch[0].pselp.write(|w| w.pselp().analog_input1());
        saadc.ch[0].pseln.write(|w| w.pseln().analog_input2());

        // Calibrate
        saadc.events_calibratedone.reset();
        saadc.tasks_calibrateoffset.write(|w| unsafe { w.bits(1) });
        while saadc.events_calibratedone.read().bits() == 0 {}

        rprintln!("calibrated");
        sample::spawn(monotonics::now()).unwrap();
        (Shared {}, Local { saadc }, init::Monotonics(mono))
    }

    #[idle]
    fn idle(_: idle::Context) -> ! {
        loop {
            // Put core to sleep until next interrupt
            asm::wfe();
        }
    }

    // Drift free periodic task at highest priority
    #[task(priority = 3, local = [saadc])]
    fn sample(cx: sample::Context, instant: fugit::TimerInstantU32<TIMER_HZ>) {
        let s = get_sample(cx.local.saadc);
        process::spawn(s).unwrap();

        // Spawn a new message with 1ms offset to spawned time
        let next_instant = instant + 1.millis();
        sample::spawn_at(next_instant, next_instant).unwrap();
    }

    #[task(priority = 2, local = [cnt: usize = 0, period: usize = 3, ptr: usize = 0, buffer: Buffer
    = [0; BUFFER_SIZE]], capacity = 2)]
    fn process(cx: process::Context, sample: i16) {
        let process::LocalResources {
            cnt,
            period,
            ptr,
            buffer,
        } = cx.local;
        *ptr = (*ptr + 1) % buffer.len();
        buffer[*ptr] = sample;

        if *cnt == 0 {
            let (err_low, err_mid, err_high) = estimate_error(*period, *ptr, buffer);
            rprintln!(
                "period {}, err_low {}, err_mid {}, err_high {}",
                period,
                err_low,
                err_mid,
                err_high
            );
            if err_mid > err_low.min(err_high) {
                if err_low < err_high {
                    *period -= 1;
                } else {
                    *period += 1;
                }
            }
            *period = (*period).clamp(2, 13); // set new period
            *cnt = *period * 5; // set counter
        }
        *cnt -= 1;
    }
}

fn estimate_error(period: usize, ptr: usize, buffer: &Buffer) -> (i32, i32, i32) {
    let mut err_low = 0;
    let mut err_mid = 0;
    let mut err_high = 0;
    for i in 0..period {
        let curr = buffer[(BUFFER_SIZE + ptr - i) % BUFFER_SIZE] as i32;
        let low = buffer[(BUFFER_SIZE + ptr - i - period - 1) % BUFFER_SIZE] as i32;
        let mid = buffer[(BUFFER_SIZE + ptr - i - period) % BUFFER_SIZE] as i32;
        let high = buffer[(BUFFER_SIZE + ptr - i - period + 1) % BUFFER_SIZE] as i32;

        err_low += (curr - low).abs();
        err_mid += (curr - mid).abs();
        err_high += (curr - high).abs();
    }
    (err_low, err_mid, err_high)
}

fn get_sample(saadc: &mut SAADC) -> i16 {
    let mut val: i16 = 0;
    saadc
        .result
        .ptr
        .write(|w| unsafe { w.ptr().bits(((&mut val) as *mut _) as u32) });
    saadc.result.maxcnt.write(|w| unsafe { w.maxcnt().bits(1) });

    // Conservative compiler fence to prevent starting the ADC before the
    // pointer and maxcount have been set.
    core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);

    saadc.tasks_start.write(|w| unsafe { w.bits(1) });
    saadc.tasks_sample.write(|w| unsafe { w.bits(1) });

    while saadc.events_end.read().bits() == 0 {}

    saadc.events_end.reset();

    // Second fence to prevent optimizations creating issues with the EasyDMA-modified `val`.
    core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);

    //rprintln!("{}, ", val);
    val
}
