// examples/rtic_hello.rs

#![no_main]
#![no_std]

use hal::pac;
use nrf52840_hal as hal;
use panic_rtt_target as _;

#[rtic::app(device = pac, dispatchers = [UARTE1])]
mod app {
    use super::*;
    use cortex_m::asm;
    use embedded_hal::digital::{OutputPin, StatefulOutputPin};
    use hal::{
        gpio::p0::Parts as P0Parts,
        gpio::{Input, p0::P0_03, Level, Output, Pin, PushPull},
        monotonic::MonotonicTimer,
        saadc::{Saadc, SaadcConfig},
    };
    use nrf52840_hal::gpio::Disconnected;
    use pac::TIMER0;

    use rtt_target::{rprintln, rtt_init_print};
    #[monotonic(binds = TIMER0, default = true)]
    type MyMono = MonotonicTimer<TIMER0, 16_000_000>;

    #[shared]
    struct Shared {}

    #[local]
    struct Local {
        led: Pin<Output<PushPull>>,
        saadc: Saadc,
        //saadc_pin: Pin<STATE>
        // saadc_pin: Pin<Disconnected>,
        saadc_pin: P0_03<Disconnected>
    }

    #[init]
    fn init(cx: init::Context) -> (Shared, Local, init::Monotonics) {
        rtt_init_print!();
        rprintln!("\n--- Hello e7020e ---\n");
        let mono = MyMono::new(cx.device.TIMER0);

        let gpios = P0Parts::new(cx.device.P0);
        let led = gpios.p0_13.into_push_pull_output(Level::High).degrade();

        // initialize saadc interface
        let saadc_config = SaadcConfig::default();
        rprintln!("gain {:?}", saadc_config.gain);
        rprintln!("oversample {:?}", saadc_config.oversample);
        rprintln!("reference {:?}", saadc_config.reference);
        rprintln!("resistor {:?}", saadc_config.resistor);
        rprintln!("time {:?}", saadc_config.time);

        let mut saadc = Saadc::new(cx.device.SAADC, saadc_config);
   //     let mut saadc_pin: Pin<STATE> = gpios.p0_03.degrade(); // the pin your analog device is connected to
        let mut saadc_pin = gpios.p0_03; // the pin your analog device is connected to

        //let _saadc_result = saadc.read_channel(&mut saadc_pin);

        blink::spawn().ok();

        (
            Shared {},
            Local {
                led,
                saadc,
                saadc_pin,
            },
            init::Monotonics(mono),
        )
    }

    #[idle]
    fn idle(_: idle::Context) -> ! {
        // let mut count: u32 = 0;
        loop {
           // rprintln!("idle {}", count);
        //  count += 1;
            // Put core to sleep until next interrupt
            asm::wfe();
        }
    }

    #[task(local = [led, saadc, 
        saadc_pin
        ])]
    fn blink(ctx: blink::Context) {
        let _saadc_result = ctx.local.saadc.read_channel(ctx.local.saadc_pin);
        rprintln!("{:?}", _saadc_result);
        let led = ctx.local.led;
        // Note this unwrap is safe since is_set_low is allways Ok
        if led.is_set_low().unwrap() {
            led.set_high().ok();
        } else {
            led.set_low().ok();
        }
        // spawn after current time + 1 milli second
        blink::spawn_after(fugit::ExtU32::millis(20)).ok();
    }
}
