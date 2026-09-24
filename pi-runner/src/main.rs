//! dudect on application-class ARM — the same registry, the other end of the
//! microarchitectural spectrum.
//!
//! The Cortex-M4 firmware measures on an in-order, cacheless core with
//! interrupts masked, where a cycle count is a deterministic function of the
//! input. A Raspberry Pi is the opposite: caches, branch prediction,
//! out-of-order execution (A72/A76), frequency scaling, and a preemptive OS.
//! The same Rust sources compiled for aarch64 take different code paths, and
//! the measurement itself becomes statistical rather than exact.
//!
//! Running both lets the paper say two things it otherwise could not:
//!   * whether the constant-time property holds across that whole spectrum, and
//!   * what a leak has to *cost* before an equipment-free experiment can see it
//!     on each platform (the sensitivity ladder — see `--ladder`).
//!
//! Output is a CSV of raw samples that `capture/import_pi.py` turns into the
//! same .npz format the boards produce, so one analysis pipeline serves both.
//!
//!   cargo build --release --features leaky
//!   sudo sysctl kernel.perf_event_paranoid=1      # once, for PMU cycles
//!   sudo cpufreq-set -g performance               # optional but advised
//!   ./target/release/pi-runner --experiment keyed --n 100000 --out pi5_keyed.csv

mod counter;

use std::fs::File;
use std::hint::black_box;
use std::io::{BufWriter, Write};

use counter::{pin_to_cpu, platform, Counter};
use probes::{Probe, MAX_TAG, PROBES};

struct Args {
    experiment: String,
    n: usize,
    probe: Option<u8>,
    out: String,
    cpu: usize,
    warmup: usize,
    board: String,
}

fn parse_args() -> Args {
    let mut a = Args {
        experiment: "verify".into(),
        n: 100_000,
        probe: None,
        out: "pi_capture.csv".into(),
        cpu: 3,
        warmup: 10_000,
        board: String::new(),
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--experiment" => a.experiment = it.next().unwrap_or_default(),
            "--n" => a.n = it.next().and_then(|v| v.parse().ok()).unwrap_or(a.n),
            "--probe" => a.probe = it.next().and_then(|v| v.parse().ok()),
            "--out" => a.out = it.next().unwrap_or(a.out),
            "--cpu" => a.cpu = it.next().and_then(|v| v.parse().ok()).unwrap_or(a.cpu),
            "--warmup" => a.warmup = it.next().and_then(|v| v.parse().ok()).unwrap_or(a.warmup),
            "--board" => a.board = it.next().unwrap_or_default(),
            "--list" => {
                for p in PROBES {
                    println!("  {:>3}  tag={:<3} key={:<3} {}", p.id, p.tag_len, p.key_len, p.name);
                }
                std::process::exit(0);
            }
            "--help" | "-h" => {
                println!("{}", env!("CARGO_PKG_NAME"));
                println!("  --experiment verify|keyed   --n <traces>   --probe <id>");
                println!("  --out <csv>   --cpu <core>   --warmup <n>   --board <label>");
                std::process::exit(0);
            }
            other => {
                eprintln!("unknown argument: {other}");
                std::process::exit(2);
            }
        }
    }
    a
}

/// A tiny xorshift so the input sequence is reproducible without pulling in a
/// dependency, and identical in structure to the boards' seeded sequence.
struct Rng(u64);
impl Rng {
    fn next_u8(&mut self) -> u8 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x >> 33) as u8
    }
    fn fill(&mut self, buf: &mut [u8]) {
        for b in buf.iter_mut() {
            *b = self.next_u8();
        }
    }
}

fn measure_verify(c: &Counter, p: &Probe, tag: &[u8]) -> u64 {
    let s = c.read();
    let ok = (p.verify)(black_box(tag));
    let e = c.read();
    let _ = black_box(ok);
    e.wrapping_sub(s)
}

fn measure_keyed(c: &Counter, p: &Probe, key: &[u8]) -> u64 {
    let s = c.read();
    let out = (p.encrypt_keyed)(black_box(key));
    let e = c.read();
    let _ = black_box(out);
    e.wrapping_sub(s)
}

fn run_probe(c: &Counter, p: &Probe, keyed: bool, n: usize, warmup: usize, rng: &mut Rng)
    -> Vec<(u8, u64)>
{
    let mut good = [0u8; MAX_TAG];
    let tag_len = (p.correct_tag)(&mut good);
    let mut fixed_tag = good;
    fixed_tag[tag_len - 1] ^= 0xFF; // long prefix match, as on the boards
    let fixed_key = [0x42u8; 32];

    let mut buf = [0u8; MAX_TAG];
    // Warm the caches and branch predictors; the first thousands of iterations
    // on an application core are not representative of steady state.
    for i in 0..warmup {
        if keyed {
            let _ = measure_keyed(c, p, &fixed_key[..p.key_len]);
        } else {
            let t: &[u8] = if i % 2 == 0 { &fixed_tag[..tag_len] } else { &good[..tag_len] };
            let _ = measure_verify(c, p, t);
        }
    }

    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let is_random = (i % 2) == 1;
        let cycles = if keyed {
            if is_random {
                rng.fill(&mut buf[..p.key_len]);
                measure_keyed(c, p, &buf[..p.key_len])
            } else {
                measure_keyed(c, p, &fixed_key[..p.key_len])
            }
        } else if is_random {
            rng.fill(&mut buf[..tag_len]);
            measure_verify(c, p, &buf[..tag_len])
        } else {
            measure_verify(c, p, &fixed_tag[..tag_len])
        };
        out.push((is_random as u8, cycles));
    }
    out
}

fn main() {
    let a = parse_args();
    let keyed = a.experiment == "keyed";
    if !keyed && a.experiment != "verify" {
        eprintln!("--experiment must be verify or keyed");
        std::process::exit(2);
    }

    if !pin_to_cpu(a.cpu) {
        eprintln!("warning: could not pin to CPU {} — expect extra variance", a.cpu);
    }
    let c = Counter::open();
    let (model, governor) = platform();
    let board = if a.board.is_empty() {
        model.to_lowercase().replace(' ', "-")
    } else {
        a.board.clone()
    };

    eprintln!("platform : {model}");
    eprintln!("counter  : {} ({})", c.source().as_str(), c.source().unit());
    eprintln!("governor : {governor}   cpu: {}   traces: {}", a.cpu, a.n);
    if governor != "performance" {
        eprintln!(
            "warning: governor is '{governor}', not 'performance'. Frequency scaling \
             adds variance that raises the smallest detectable leak."
        );
    }

    let f = File::create(&a.out).expect("cannot create output file");
    let mut w = BufWriter::new(f);
    writeln!(w, "# board={board}").unwrap();
    writeln!(w, "# model={model}").unwrap();
    writeln!(w, "# counter={}", c.source().as_str()).unwrap();
    writeln!(w, "# counter_unit={}", c.source().unit()).unwrap();
    writeln!(w, "# counter_hz={}", c.hz().map(|h| h.to_string()).unwrap_or_default()).unwrap();
    writeln!(w, "# governor={governor}").unwrap();
    writeln!(w, "# experiment={}", a.experiment).unwrap();
    writeln!(w, "probe,label,cycles").unwrap();

    let mut rng = Rng(0xC0FFEE);
    let targets: Vec<&Probe> = PROBES
        .iter()
        .filter(|p| a.probe.map_or(true, |id| p.id == id))
        .collect();
    if targets.is_empty() {
        eprintln!("no such probe id");
        std::process::exit(2);
    }

    for p in targets {
        let samples = run_probe(&c, p, keyed, a.n, a.warmup, &mut rng);
        let fixed: Vec<u64> = samples.iter().filter(|s| s.0 == 0).map(|s| s.1).collect();
        let rand: Vec<u64> = samples.iter().filter(|s| s.0 == 1).map(|s| s.1).collect();
        let mf = fixed.iter().sum::<u64>() as f64 / fixed.len() as f64;
        let mr = rand.iter().sum::<u64>() as f64 / rand.len() as f64;
        eprintln!("  {:<26} fixed={mf:>12.1} random={mr:>12.1}", p.name);
        for (label, cycles) in samples {
            writeln!(w, "{},{label},{cycles}", p.name).unwrap();
        }
    }
    w.flush().unwrap();
    eprintln!("wrote {}", a.out);
}
