use std::{path::Path, time::Instant};

use ionic::{IonReader, Range, ReadOptions};

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: bench_window <file.ion> [sample_count]");
        std::process::exit(1);
    };
    let sample_count = args
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(200);

    if let Err(message) = run(&path, sample_count) {
        eprintln!("error: {message}");
        std::process::exit(1);
    }
}

fn run(path: &str, sample_count: usize) -> Result<(), String> {
    let mut ion = IonReader::open(Path::new(path), &ReadOptions::default())
        .map_err(|error| format!("cannot open {path}: {error}"))?;

    let total_spectra = ion.spectrum_count() as usize;
    if total_spectra == 0 {
        return Err("file has no spectra".into());
    }
    let sample = sample_count.min(total_spectra);

    let first = read_full(&mut ion, 0)?;
    if first.is_empty() {
        return Err("first spectrum has no m/z data".into());
    }
    let low_mz = first.iter().copied().fold(f64::INFINITY, f64::min);
    let high_mz = first.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let span = high_mz - low_mz;
    let window_low = low_mz + span * 0.49;
    let window_high = low_mz + span * 0.51;

    warm_cache(&mut ion, sample)?;
    let full = measure(&mut ion, sample, 0.0, f64::MAX)?;
    let window = measure(&mut ion, sample, window_low, window_high)?;

    let full_avg = full.micros / sample as f64;
    let window_avg = window.micros / sample as f64;
    let speedup = if window_avg > 0.0 {
        full_avg / window_avg
    } else {
        0.0
    };

    println!("file            {path}");
    println!("spectra total   {total_spectra}");
    println!("spectra sampled {sample}");
    println!("m/z range       {low_mz:.2} .. {high_mz:.2}");
    println!("window          {window_low:.2} .. {window_high:.2}  (middle 2%)");
    println!(
        "full read       {full_avg:.1} us per spectrum   {} points",
        full.points
    );
    println!(
        "window read     {window_avg:.1} us per spectrum   {} points",
        window.points
    );
    println!("speedup         {speedup:.1}x");
    Ok(())
}

struct Reading {
    micros: f64,
    points: usize,
}

fn read_full(ion: &mut IonReader, index: usize) -> Result<Vec<f64>, String> {
    ion.read_window(
        index,
        Range {
            from: 0.0,
            to: f64::MAX,
        },
    )
    .map(|window| window.x.to_f64())
    .map_err(|error| format!("cannot read spectrum {index}: {error}"))
}

fn warm_cache(ion: &mut IonReader, sample: usize) -> Result<(), String> {
    for index in 0..sample {
        read_full(ion, index)?;
    }
    Ok(())
}

fn measure(ion: &mut IonReader, sample: usize, low: f64, high: f64) -> Result<Reading, String> {
    let start = Instant::now();
    let mut points = 0usize;
    for index in 0..sample {
        let window = ion
            .read_window(
                index,
                Range {
                    from: low,
                    to: high,
                },
            )
            .map_err(|error| format!("cannot read spectrum {index}: {error}"))?;
        points += window.x.len();
    }
    let micros = start.elapsed().as_secs_f64() * 1_000_000.0;
    Ok(Reading { micros, points })
}
