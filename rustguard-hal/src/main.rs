#![no_std]
#![no_main]

//! rustguard-hal — STM32L476 bare-metal firmware integration
//!
//! This crate demonstrates cross-compilation of rustguard-pap to a
//! Cortex-M4 target (STM32L476RG). It requires the physical board to run;
//! it compiles with `cargo check --target thumbv7em-none-eabihf` on any machine.
//!
//! ## Build
//! ```bash
//! cargo build --release --target thumbv7em-none-eabihf
//! ```
//!
//! ## Flash (requires probe-rs + physical board)
//! ```bash
//! cargo run --release --target thumbv7em-none-eabihf
//! ```

use cortex_m::peripheral::DWT;
use cortex_m_rt::entry;
use panic_halt as _;
use stm32l4xx_hal::{
    prelude::*,
    serial::{Config, Serial},
    stm32,
};
use rustguard_pap::PacketBuilder;
use core::fmt::Write;

#[entry]
fn main() -> ! {
    let dp = stm32::Peripherals::take().unwrap();
    let mut cp = cortex_m::Peripherals::take().unwrap();

    let mut flash = dp.FLASH.constrain();
    let mut rcc   = dp.RCC.constrain();
    let mut pwr   = dp.PWR.constrain(&mut rcc.apb1r1);

    // 80 MHz clock (Section VI.A of the paper)
    let clocks = rcc.cfgr
        .sysclk(80.mhz())
        .pclk1(80.mhz())
        .pclk2(80.mhz())
        .freeze(&mut flash.acr, &mut pwr);

    // Enable DWT cycle counter for cycle-precise benchmarking
    cp.DCB.enable_trace();
    DWT::unlock();
    cp.DWT.enable_cycle_counter();

    // UART2 on PA2/PA3 at 115200 baud
    let mut gpioa = dp.GPIOA.split(&mut rcc.ahb2);
    let tx = gpioa.pa2.into_alternate(&mut gpioa.moder, &mut gpioa.otyper, &mut gpioa.afrl);
    let rx = gpioa.pa3.into_alternate(&mut gpioa.moder, &mut gpioa.otyper, &mut gpioa.afrl);

    let mut serial = Serial::usart2(
        dp.USART2,
        (tx, rx),
        Config::default().baudrate(115_200.bps()),
        clocks,
        &mut rcc.apb1r1,
    );

    writeln!(serial, "RustGuard HAL booting on STM32L476 @ 80 MHz").unwrap();
    writeln!(serial, "ASCON-128 / RustGuard-PAP").unwrap();

    // Pre-shared key (in real deployment: provisioned into flash at manufacture)
    let key = [0x42u8; 16];
    let mut builder = PacketBuilder::new(key, 0);

    // Representative 32-byte sensor payload (matches N-BaIoT mean 34.7 B)
    let payload = b"Temperature: 22.4C  Humidity:65%";

    loop {
        // Measure full PAP packet construction using DWT cycle counter
        let start = DWT::cycle_count();
        let packet = builder.build_packet(payload, 0x1011, 1, 1);
        let end   = DWT::cycle_count();

        let cycles  = end.wrapping_sub(start);
        let cpb     = cycles as f32 / packet.len() as f32;

        writeln!(
            serial,
            "Packet len={} bytes | cycles={} | cyc/B={:.2}",
            packet.len(), cycles, cpb
        ).unwrap();

        // 10-second sleep (800M cycles @ 80 MHz)
        cortex_m::asm::delay(800_000_000);
    }
}
