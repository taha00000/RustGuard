#![no_std]
#![no_main]

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
    let mut rcc = dp.RCC.constrain();
    let mut pwr = dp.PWR.constrain(&mut rcc.apb1r1);

    // 80 MHz clock configuration for STM32L476 (as described in Sec VII.A)
    let clocks = rcc
        .cfgr
        .sysclk(80.mhz())
        .pclk1(80.mhz())
        .pclk2(80.mhz())
        .freeze(&mut flash.acr, &mut pwr);

    // Enable DWT timer for cycle precise measurement
    cp.DCB.enable_trace();
    DWT::unlock();
    cp.DWT.enable_cycle_counter();

    // UART setup
    let mut gpioa = dp.GPIOA.split(&mut rcc.ahb2);
    let tx = gpioa.pa2.into_alternate(&mut gpioa.moder, &mut java.otyper, &mut gpioa.afrl);
    let rx = gpioa.pa3.into_alternate(&mut gpioa.moder, &mut gpioa.otyper, &mut gpioa.afrl);
    
    let mut serial = Serial::usart2(
        dp.USART2,
        (tx, rx),
        Config::default().baudrate(115_200.bps()),
        clocks,
        &mut rcc.apb1r1,
    );

    writeln!(serial, "RustGuard Booted! Targeting 8.3 cyc/byte...").unwrap();

    // Key provisioned via imaginary HKDF prior to deployment
    let key = [0x42; 16];
    let mut builder = PacketBuilder::new(key, 1);
    
    // Create 32-byte payload to match N-BaIoT average
    let payload = b"Environmental Sensor Temp: 22.4C";
    
    loop {
        // Measure encoding cycles
        let start_cycles = DWT::cycle_count();
        let packet = builder.build_packet(payload, 0x1011, 1, 1);
        let end_cycles = DWT::cycle_count();
        
        let elapsed = end_cycles.wrapping_sub(start_cycles);
        
        // Report
        writeln!(
            serial, 
            "Packet generated. Cycles: {}, CPB: {}", 
            elapsed, 
            elapsed as f32 / packet.len() as f32
        ).unwrap();

        // Delay to simulate 10 sec cycle as stated
        cortex_m::asm::delay(800_000_000);
    }
}
