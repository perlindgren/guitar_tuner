// examples/rtic_saadc_raw.rs

#![no_main]
#![no_std]

use hal::pac;
use nrf52840_hal as hal;
use panic_rtt_target as _;

#[rtic::app(device = pac, dispatchers = [UARTE1])]
mod app {
    use super::*;
    use cortex_m::asm;

    use pac::saadc::{ch::config::*, oversample::OVERSAMPLE_A, resolution::VAL_A};
    use rtt_target::{rprintln, rtt_init_print};

    #[shared]
    struct Shared {}

    #[local]
    struct Local {}

    #[init]
    fn init(cx: init::Context) -> (Shared, Local, init::Monotonics) {
        rtt_init_print!();
        rprintln!("\n--- saadc raw ---\n");

        let saadc = cx.device.SAADC;

        saadc.enable.write(|w| w.enable().enabled());
        saadc.resolution.write(|w| w.val().variant(VAL_A::_14BIT));
        saadc
            .oversample
            .write(|w| w.oversample().variant(OVERSAMPLE_A::BYPASS));
        saadc.samplerate.write(|w| w.mode().task());

        saadc.ch[0].config.write(|w| {
            // w.refsel().variant(REFSEL_A::INTERNAL);
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

        let mut delay = cortex_m::delay::Delay::new(cx.core.SYST, 64_000_000);

        loop {
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

            rprintln!("{}, ", val);

            delay.delay_us(1000);
        }
        #[allow(unreachable_code)]
        (Shared {}, Local {}, init::Monotonics())
    }

    #[idle]
    fn idle(_: idle::Context) -> ! {
        loop {
            // Put core to sleep until next interrupt
            asm::wfe();
        }
    }
}
