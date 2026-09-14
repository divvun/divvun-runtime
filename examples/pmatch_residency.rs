//! What one loaded pmatch archive costs, and what a second reader of it costs.
//!
//! The four numbers a decision on mmap-backed tables turns on:
//!   * `load1`   — time and resident cost of loading ONE core.
//!   * `states N`— resident cost of N run states over that one core, and the
//!                 time to make one.
//!   * `loads N` — resident cost of N separate private loads, for contrast.
//!
//! Loading goes through the same two calls `modules::hfst::load_tokenizer_core`
//! makes — `MemoryMappedFile::open_ro` + `Segment`, then
//! `PmatchCore::from_stream` over the mapped bytes — so the numbers are the
//! runtime's own load path, not a synthetic one.
//!
//! Resident size is read from `ps -o rss=` (KiB) at each checkpoint; run the
//! whole thing under `/usr/bin/time -l` for the peak.
//!
//! Usage:
//!   cargo run --release --example pmatch_residency -- <mode> <model.pmhfst> [n]

use std::sync::Arc;
use std::time::Instant;

use hfst::pmatch::PmatchContainer;
use hfst::pmatch_core::PmatchCore;
use hfst::pmatch_tokenize::{
    OutputFormat, TokenizeInputSettings, TokenizeSettings, process_input_stream,
};
use mmap_io::{MemoryMappedFile, segment::Segment};

const SENTENCE: &str = "Mun lean Márjá ja mun ásan Guovdageainnus.";

/// Resident set size in KiB, as the kernel reports it to `ps`.
fn rss_kb() -> u64 {
    let pid = std::process::id().to_string();
    let out = std::process::Command::new("/bin/ps")
        .args(["-o", "rss=", "-p", &pid])
        .output()
        .expect("ps must run");
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .expect("ps prints rss as a number")
}

fn mib(kb: u64) -> f64 {
    kb as f64 / 1024.0
}

fn mark(label: &str, base: u64) {
    let now = rss_kb();
    println!(
        "  {label:<34} rss {:>9.1} MiB   (delta {:>+9.2} MiB)",
        mib(now),
        mib(now) as f64 - mib(base)
    );
}

fn load_core(path: &str) -> Result<Arc<PmatchCore>, Box<dyn std::error::Error>> {
    let mmap = Arc::new(MemoryMappedFile::open_ro(path)?);
    let len = mmap.len();
    let segment = Segment::new(mmap, 0, len)?;
    let bytes = segment.as_slice()?;
    let mut cursor = std::io::Cursor::new(bytes);
    Ok(Arc::new(PmatchCore::from_stream(&mut cursor)?))
}

fn run_state(core: Arc<PmatchCore>) -> PmatchContainer {
    let mut container = PmatchContainer::from_core(core);
    container.set_verbose(false);
    container.set_single_codepoint_tokenization(true);
    container
}

fn giellacg_settings() -> TokenizeSettings {
    TokenizeSettings {
        output_format: OutputFormat::giellacg,
        print_weights: true,
        print_all: true,
        dedupe: true,
        max_weight_classes: i32::MAX,
        tokenize_multichar: false,
        ..TokenizeSettings::default()
    }
}

fn tokenize(container: &mut PmatchContainer, settings: &TokenizeSettings, input: &str) -> String {
    let input_settings = TokenizeInputSettings {
        superblanks: false,
        verbose: false,
        ..TokenizeInputSettings::default()
    };
    let mut output: Vec<u8> = Vec::new();
    let mut msg = std::io::sink();
    let mut reader = std::io::Cursor::new(input.as_bytes());
    process_input_stream(
        container,
        &mut reader,
        &mut output,
        &mut msg,
        settings,
        &input_settings,
    );
    String::from_utf8_lossy(&output).into_owned()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let mode = args
        .next()
        .ok_or("usage: pmatch_residency <load1|states|loads> <model.pmhfst> [n]")?;
    let path = args.next().ok_or("missing model path")?;
    let n: usize = args.next().unwrap_or_else(|| "8".into()).parse()?;

    let settings = giellacg_settings();
    let baseline = rss_kb();
    println!("baseline rss {:.1} MiB", mib(baseline));

    match mode.as_str() {
        "load1" => {
            let t = Instant::now();
            let core = load_core(&path)?;
            let load = t.elapsed();
            mark("after 1 core loaded", baseline);
            println!("  load wall time                     {:?}", load);

            let t = Instant::now();
            let mut state = run_state(Arc::clone(&core));
            let create = t.elapsed();
            mark("after 1 run state", baseline);
            println!("  run-state creation                 {:?}", create);

            let out = tokenize(&mut state, &settings, SENTENCE);
            mark("after 1 run state tokenized", baseline);
            println!("  output bytes                       {}", out.len());
        }
        "states" => {
            let core = load_core(&path)?;
            let loaded = rss_kb();
            mark("after 1 core loaded", baseline);

            // Time N creations as a batch as well as reporting the first, so a
            // per-state cost below timer resolution is still visible.
            let t = Instant::now();
            let mut states: Vec<PmatchContainer> =
                (0..n).map(|_| run_state(Arc::clone(&core))).collect();
            let create_all = t.elapsed();
            let fresh = rss_kb();
            mark(&format!("after {n} fresh run states"), loaded);
            println!(
                "  {n} creations                       {:?}  ({:?} each)",
                create_all,
                create_all / n as u32
            );

            for state in states.iter_mut() {
                let _ = tokenize(state, &settings, SENTENCE);
            }
            mark(&format!("after {n} states tokenized"), fresh);
            println!(
                "  per state, used   {:>+9.3} MiB",
                (mib(rss_kb()) - mib(loaded)) / n as f64
            );
            // Keep them alive past the last measurement.
            println!("  states held: {}", states.len());
        }
        "loads" => {
            let mut cores: Vec<Arc<PmatchCore>> = Vec::new();
            let mut prev = baseline;
            for i in 1..=n {
                let t = Instant::now();
                cores.push(load_core(&path)?);
                let load = t.elapsed();
                let now = rss_kb();
                mark(&format!("after private load {i}"), prev);
                println!("  load {i} wall time                   {:?}", load);
                prev = now;
            }
            println!("  cores held: {}", cores.len());
        }
        other => return Err(format!("unknown mode {other:?}").into()),
    }

    Ok(())
}
