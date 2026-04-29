#![no_std]
#![no_main]

//! rustguard-hal-tiva — RustGuard Benchmark Firmware for TM4C123GH6PM
//!
//! Measures ASCON-128 encryption/decryption/PAP performance using the ARM
//! Data Watchpoint and Trace (DWT) cycle counter on the Tiva C LaunchPad.
//!
//! ## Hardware
//! - Board : EK-TM4C123GXL (Tiva C LaunchPad)
//! - MCU   : TM4C123GH6PM, ARM Cortex-M4F @ 80 MHz
//! - Flash : 256 KB  |  SRAM: 32 KB
//! - UART  : UART0 on PA0/PA1, 115200 baud (appears as virtual COM over USB)
//!
//! ## Build & Flash
//! ```bash
//! rustup target add thumbv7em-none-eabihf
//! cargo build --release --target thumbv7em-none-eabihf
//!
//! # Flash via OpenOCD (requires OpenOCD ≥ 0.12 with TI ICDI support):
//! openocd -f interface/ti-icdi.cfg -f target/tm4c123g.cfg \
//!   -c "program target/thumbv7em-none-eabihf/release/rustguard-hal-tiva \
//!       verify reset exit"
//! ```
//!
//! ## Output format (115200 8N1 on virtual COM port)
//! ```
//! RustGuard v0.1 on TM4C123GH6PM Cortex-M4F @ 80 MHz
//! SECTION:ENCRYPT_LATENCY
//! ENC_LAT 8   <mean_cycles>  <min_cycles>  <max_cycles>
//! ...
//! SECTION:PERMUTATION
//! PERM p6  <cycles>
//! PERM p12 <cycles>
//! ```

use core::fmt::Write;

use cortex_m::peripheral::DWT;
use cortex_m_rt::entry;
use panic_halt as _;
use tm4c123x_hal::{self as hal, prelude::*};

use rustguard_core::{ascon_aead_encrypt, ascon_aead_decrypt, ascon_hash, ascon_p, State};
use rustguard_pap::PacketBuilder;

// ── Benchmark parameters ──────────────────────────────────────────────────────
const WARMUP: u32 = 50;
const ITERS:  u32 = 500;   // 500 iters × 8 sizes = 4,000 DWT readings total
const PAYLOAD_SIZES: [usize; 7] = [8, 16, 32, 64, 128, 256, 512];

// Fixed key, nonce, AD for all benchmarks
const KEY:   [u8; 16] = [0x42u8; 16];
const NONCE: [u8; 16] = [0xAAu8; 16];
const AD:    [u8;  8] = [0x01u8;  8];  // 8-byte header+seq (PAP structure)

// ── Cycle counting helpers ────────────────────────────────────────────────────

/// Reset DWT CYCCNT to zero and return the current count.
#[inline(always)]
fn cycles_reset() -> u32 {
    unsafe { (*cortex_m::peripheral::DWT::PTR).cyccnt.write(0) };
    0
}

/// Read the DWT cycle counter.
#[inline(always)]
fn cycles_now() -> u32 {
    DWT::cycle_count()
}

// ── Benchmark: encrypt one payload size ──────────────────────────────────────
fn bench_encrypt(payload: &[u8]) -> (u32, u32, u32) {
    let mut ct  = [0u8; 512];
    let mut tag = [0u8; 16];

    // Warmup
    for _ in 0..WARMUP {
        ascon_aead_encrypt(&KEY, &NONCE, &AD, payload, &mut ct[..payload.len()], &mut tag);
    }

    let mut min = u32::MAX;
    let mut max = 0u32;
    let mut sum = 0u64;

    for _ in 0..ITERS {
        cycles_reset();
        let t0 = cycles_now();
        ascon_aead_encrypt(&KEY, &NONCE, &AD, payload, &mut ct[..payload.len()], &mut tag);
        let elapsed = cycles_now().wrapping_sub(t0);
        if elapsed < min { min = elapsed; }
        if elapsed > max { max = elapsed; }
        sum += elapsed as u64;
    }

    let mean = (sum / ITERS as u64) as u32;
    (mean, min, max)
}

// ── Benchmark: decrypt one payload size ──────────────────────────────────────
fn bench_decrypt(payload: &[u8]) -> (u32, u32, u32) {
    let mut ct  = [0u8; 512];
    let mut tag = [0u8; 16];
    let mut pt  = [0u8; 512];
    // Encrypt first to get valid ciphertext
    ascon_aead_encrypt(&KEY, &NONCE, &AD, payload, &mut ct[..payload.len()], &mut tag);

    for _ in 0..WARMUP {
        ascon_aead_decrypt(&KEY, &NONCE, &AD, &ct[..payload.len()], &mut pt[..payload.len()], &tag);
    }

    let mut min = u32::MAX;
    let mut max = 0u32;
    let mut sum = 0u64;

    for _ in 0..ITERS {
        cycles_reset();
        let t0 = cycles_now();
        ascon_aead_decrypt(&KEY, &NONCE, &AD, &ct[..payload.len()], &mut pt[..payload.len()], &tag);
        let elapsed = cycles_now().wrapping_sub(t0);
        if elapsed < min { min = elapsed; }
        if elapsed > max { max = elapsed; }
        sum += elapsed as u64;
    }

    let mean = (sum / ITERS as u64) as u32;
    (mean, min, max)
}

// ── Benchmark: full PAP build_packet ─────────────────────────────────────────
fn bench_pap(payload: &[u8]) -> (u32, u32, u32) {
    let mut builder = PacketBuilder::new(KEY, 0);

    for _ in 0..WARMUP {
        let _ = builder.build_packet(payload, 0x0001, 1, 1);
    }

    let mut min = u32::MAX;
    let mut max = 0u32;
    let mut sum = 0u64;

    for _ in 0..ITERS {
        cycles_reset();
        let t0 = cycles_now();
        let _ = builder.build_packet(payload, 0x0001, 1, 1);
        let elapsed = cycles_now().wrapping_sub(t0);
        if elapsed < min { min = elapsed; }
        if elapsed > max { max = elapsed; }
        sum += elapsed as u64;
    }

    let mean = (sum / ITERS as u64) as u32;
    (mean, min, max)
}

// ── Benchmark: ASCON permutation ─────────────────────────────────────────────
fn bench_permutation(rounds: usize) -> (u32, u32, u32) {
    let mut s = State { x0: 1, x1: 2, x2: 3, x3: 4, x4: 5 };

    for _ in 0..WARMUP {
        ascon_p(&mut s, rounds);
    }

    let mut min = u32::MAX;
    let mut max = 0u32;
    let mut sum = 0u64;

    for _ in 0..ITERS {
        cycles_reset();
        let t0 = cycles_now();
        ascon_p(&mut s, rounds);
        let elapsed = cycles_now().wrapping_sub(t0);
        if elapsed < min { min = elapsed; }
        if elapsed > max { max = elapsed; }
        sum += elapsed as u64;
    }

    let mean = (sum / ITERS as u64) as u32;
    (mean, min, max)
}

// ── Benchmark: ASCON-HASH ─────────────────────────────────────────────────────
fn bench_hash(data: &[u8]) -> (u32, u32, u32) {
    let mut out = [0u8; 32];

    for _ in 0..WARMUP {
        ascon_hash(data, &mut out);
    }

    let mut min = u32::MAX;
    let mut max = 0u32;
    let mut sum = 0u64;

    for _ in 0..ITERS {
        cycles_reset();
        let t0 = cycles_now();
        ascon_hash(data, &mut out);
        let elapsed = cycles_now().wrapping_sub(t0);
        if elapsed < min { min = elapsed; }
        if elapsed > max { max = elapsed; }
        sum += elapsed as u64;
    }

    let mean = (sum / ITERS as u64) as u32;
    (mean, min, max)
}

// ── Correctness self-test ─────────────────────────────────────────────────────
fn self_test() -> bool {
    // Round-trip test: encrypt then decrypt, verify plaintext recovery
    let pt  = b"RustGuard ASCON-128 self-test 32";
    let ad  = b"device=0x0001";
    let mut ct  = [0u8; 32];
    let mut tag = [0u8; 16];
    let mut rec = [0u8; 32];

    ascon_aead_encrypt(&KEY, &NONCE, ad, pt, &mut ct, &mut tag);
    let ok = ascon_aead_decrypt(&KEY, &NONCE, ad, &ct, &mut rec, &tag);
    if !ok { return false; }
    if &rec != pt { return false; }

    // Tamper test: corrupted ciphertext must NOT verify
    ct[0] ^= 0x01;
    let bad = ascon_aead_decrypt(&KEY, &NONCE, ad, &ct, &mut rec, &tag);
    if bad { return false; }

    // PAP round-trip
    let mut builder = PacketBuilder::new(KEY, 0);
    let payload = b"IoT sensor reading test";
    let packet  = builder.build_packet(payload, 0x0001, 1, 1);
    let verifier = PacketBuilder::new(KEY, 0);
    let mut out  = [0u8; 64];
    let result   = verifier.unwrap_packet(&packet, 0, &mut out);
    if result.is_err() { return false; }

    true
}

// ── Main ──────────────────────────────────────────────────────────────────────
#[entry]
fn main() -> ! {
    let p  = hal::Peripherals::take().unwrap();
    let cp = cortex_m::Peripherals::take().unwrap();

    // ── Clock: 80 MHz via PLL ─────────────────────────────────────────────
    let mut sc = p.SYSCTL.constrain();
    sc.clock_setup.oscillator = hal::sysctl::Oscillator::Main(
        hal::sysctl::CrystalFrequency::_16mhz,
        hal::sysctl::SystemClock::UsePll(hal::sysctl::PllOutputFrequency::_80_00mhz),
    );
    let clocks = sc.clock_setup.freeze();

    // ── Enable DWT cycle counter ──────────────────────────────────────────
    let mut core = cp;
    core.DCB.enable_trace();
    core.DWT.enable_cycle_counter();

    // ── UART0 @ 115200 baud on PA0/PA1 ────────────────────────────────────
    let mut porta = p.GPIO_PORTA.split(&sc.power_control);
    let uart0 = hal::serial::Serial::uart0(
        p.UART0,
        porta.pa1.into_af_push_pull::<hal::gpio::AF1>(&mut porta.control),
        porta.pa0.into_af_push_pull::<hal::gpio::AF1>(&mut porta.control),
        (),
        (),
        115_200_u32.bps(),
        hal::serial::NewlineMode::SwapLFtoCRLF,
        &clocks,
        &sc.power_control,
    );
    let (mut tx, _rx) = uart0.split();

    writeln!(tx, "RustGuard v0.1 on TM4C123GH6PM Cortex-M4F @ 80 MHz").ok();
    writeln!(tx, "Iterations: {ITERS}  Warmup: {WARMUP}").ok();
    writeln!(tx, "All cycles at 80 MHz: 1 cycle = 12.5 ns").ok();
    writeln!(tx, "").ok();

    // ── Self-test ─────────────────────────────────────────────────────────
    writeln!(tx, "SECTION:SELF_TEST").ok();
    if self_test() {
        writeln!(tx, "SELF_TEST PASS").ok();
    } else {
        writeln!(tx, "SELF_TEST FAIL — HALTING").ok();
        loop {}
    }
    writeln!(tx, "").ok();

    // ── Permutation benchmarks ────────────────────────────────────────────
    writeln!(tx, "SECTION:PERMUTATION").ok();
    let (m6,  mn6,  mx6)  = bench_permutation(6);
    let (m12, mn12, mx12) = bench_permutation(12);
    // cycles/byte is not applicable to permutation; report raw cycles
    writeln!(tx, "PERM p6  mean={m6}  min={mn6}  max={mx6}  cyc").ok();
    writeln!(tx, "PERM p12 mean={m12} min={mn12} max={mx12} cyc").ok();
    // Annotate with nanoseconds (1 cycle = 12.5 ns @ 80 MHz)
    writeln!(tx, "PERM p6  mean_ns={}", m6  * 125 / 10).ok();
    writeln!(tx, "PERM p12 mean_ns={}", m12 * 125 / 10).ok();
    writeln!(tx, "").ok();

    // ── Encrypt benchmarks ────────────────────────────────────────────────
    writeln!(tx, "SECTION:ENCRYPT_LATENCY").ok();
    writeln!(tx, "# format: ENC_LAT size_bytes mean_cyc min_cyc max_cyc mean_ns cyc_per_byte").ok();
    for &sz in &PAYLOAD_SIZES {
        let payload = [0xBEu8; 512];
        let (m, mn, mx) = bench_encrypt(&payload[..sz]);
        let ns  = m as u64 * 125 / 10;   // cycles → ns at 80 MHz
        let cpb = (m as u64 * 100) / sz as u64;  // cycles/byte × 100
        writeln!(tx, "ENC_LAT {sz:3}  mean={m:6}  min={mn:6}  max={mx:6}  ns={ns:6}  cpb={}.{:02}",
                 cpb / 100, cpb % 100).ok();
    }
    writeln!(tx, "").ok();

    // ── Decrypt benchmarks ────────────────────────────────────────────────
    writeln!(tx, "SECTION:DECRYPT_LATENCY").ok();
    writeln!(tx, "# format: DEC_LAT size_bytes mean_cyc min_cyc max_cyc mean_ns").ok();
    for &sz in &PAYLOAD_SIZES {
        let payload = [0xBEu8; 512];
        let (m, mn, mx) = bench_decrypt(&payload[..sz]);
        let ns = m as u64 * 125 / 10;
        writeln!(tx, "DEC_LAT {sz:3}  mean={m:6}  min={mn:6}  max={mx:6}  ns={ns:6}").ok();
    }
    writeln!(tx, "").ok();

    // ── PAP build_packet benchmarks ───────────────────────────────────────
    writeln!(tx, "SECTION:PAP_LATENCY").ok();
    writeln!(tx, "# format: PAP_LAT size_bytes mean_cyc min_cyc max_cyc mean_ns").ok();
    for &sz in &PAYLOAD_SIZES {
        let payload = [0xBEu8; 512];
        let (m, mn, mx) = bench_pap(&payload[..sz]);
        let ns = m as u64 * 125 / 10;
        writeln!(tx, "PAP_LAT {sz:3}  mean={m:6}  min={mn:6}  max={mx:6}  ns={ns:6}").ok();
    }
    writeln!(tx, "").ok();

    // ── ASCON-HASH benchmarks ─────────────────────────────────────────────
    writeln!(tx, "SECTION:HASH_LATENCY").ok();
    for &sz in &[8usize, 32, 64] {
        let data = [0x55u8; 64];
        let (m, mn, mx) = bench_hash(&data[..sz]);
        let ns = m as u64 * 125 / 10;
        writeln!(tx, "HASH_LAT {sz:3}  mean={m:6}  min={mn:6}  max={mx:6}  ns={ns:6}").ok();
    }
    writeln!(tx, "").ok();

    writeln!(tx, "SECTION:COMPLETE").ok();
    writeln!(tx, "Benchmark complete. Paste output into results/raw/benchmark_tm4c.txt").ok();

    // ── Idle loop: blink LED to show we finished ──────────────────────────
    let mut portf = p.GPIO_PORTF.split(&sc.power_control);
    // Red LED on PF1
    let mut led = portf.pf1.into_push_pull_output();
    loop {
        led.set_high();
        cortex_m::asm::delay(4_000_000);   // ~50 ms @ 80 MHz
        led.set_low();
        cortex_m::asm::delay(4_000_000);
    }
}
