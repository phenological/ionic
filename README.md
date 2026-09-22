# Ionic

<img src="assets/ion-glyph.svg" alt="ionic" width="110" align="right">

[![CI](https://github.com/phenological/ionic/actions/workflows/rust-tests.yml/badge.svg)](https://github.com/phenological/ionic/actions/workflows/rust-tests.yml)
[![DOI](https://zenodo.org/badge/DOI/10.5281/zenodo.22700673.svg)](https://doi.org/10.5281/zenodo.22700673)

**A streamable binary file format for mass-spectrometry profiling and imaging data, and
the Rust library and CLI that read and write it.**

Ionic converts losslessly to and from mzML, depends on nothing but standard byte
operations, and compiles to WebAssembly, so the same reader runs in a server, a notebook
or a browser tab.

### Install

```bash
cargo add ionic --git https://github.com/phenological/ionic --branch main
```

## Why

Metabolic phenotyping has reached a scale at which the volume of data, not the
sophistication of the model, limits what can be learned from it. Population-scale
mass-spectrometry cohorts are the substrate for machine learning, which must iterate over
the entire corpus, so the size and speed at which data can be read set the ceiling on what
is feasible. At the same time, untargeted studies nominate thousands of features whose
underlying peaks are almost never inspected, because with conventional formats that means
retrieving gigabytes and loading a heavyweight tool per feature.

The community exchange format for mass-spectrometry data (mzML), used in proteomics,
metabolic profiling and imaging, is text-based and thus too large for long-term storage,
and has to be read in full before any spectrum can be reached. More compact binary
alternatives such as mzMLb solve the size problem but are built on general-purpose storage
libraries such as HDF5, which tie a file to a particular software stack and cannot
reasonably be compiled to a lightweight target such as WebAssembly.

Ionic stores the same information as native numeric types in independently compressed
Zstandard blocks, addressed through small fixed-width directories. A reader searches an
index, finds where a spectrum lives, requests that byte range, and leaves the rest of the
file untouched and compressed. Over HTTP, that is a range request; on disk, it is a seek.

## [CLI →](crates/cli/README.MD)

A command-line tool for converting mzML files to Ionic. See the [CLI](crates/cli/README.MD) for installation and commands.

## Usage

| Item | Available | What it does |
|---|---|---|
| `ionic::read` / `ionic::write` | std | Whole `.ion` file to/from an `MzML`, in one call. |
| `ionic::convert` | everywhere | Convert an in-memory buffer between mzML and Ionic. |
| `ionic::convert_file` | std | Convert between two paths, streaming to disk. |
| `IonReader::open` | std | Open a `.ion` file by path (memory-mapped). |
| `IonReader::from_bytes` | everywhere | Open a `.ion` file already in memory. |
| `IonReader::new` | everywhere | Open from any `ionic::source::ReadBytes` (partial/remote reads). |
| `IonWriter::create` | std | Write a new `.ion` file by path. |
| `IonWriter::to` | everywhere | Write into any `ionic::source::WriteBytes` sink, such as a `Vec<u8>`. |
| `ionic::mzml` | everywhere | mzML types and parser/serializer: `MzML`, `Spectrum`, `Chromatogram`, `NumericArray`, `parse_mzml`, `bin_to_mzml`, ... |
| `ionic::source` | everywhere | Partial/remote-read building blocks: `ReadBytes`, `WriteBytes`, `ByteRange`, `CallbackSource`, `header_ranges`, `merge_ranges`. |
| `ionic::format` | everywhere | File-format constants used by tooling: `CURRENT_VERSION`, `HEADER_SIZE`, `FILE_SIGNATURE`, `is_supported`, ... |

"std" means the item needs a real filesystem and is not available on `wasm32-unknown-unknown`; "everywhere" means it also works there.

### Read, one call

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mzml = ionic::read("run.ion")?;
    let count = mzml.run.spectrum_list.map_or(0, |list| list.spectra.len());
    println!("{count} spectra");
    Ok(())
}
```

### Reader with options

```rust
use ionic::{ArrayKind, IonReader, ReadOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = ReadOptions {
        parallel: false,
        ..ReadOptions::default()
    };
    let mut reader = IonReader::open("run.ion", &options)?;
    let mz = reader.array(0, ArrayKind::Mz)?;
    let intensity = reader.array(0, ArrayKind::Intensity)?;
    println!("{} points", mz.len().min(intensity.len()));
    Ok(())
}
```

`ReadOptions::default()`:

| Field | Default |
|---|---|
| `max_cached_bytes` | `256 * 1024 * 1024` (256 MiB decoded-block cache) |
| `verify_checksums` | `true` |
| `parallel` | `true` |
| `decompression_limit` | `DecompressionLimit::default()` (2 GiB uncompressed cap) |

### Write, one call

```rust
use ionic::WriteOptions;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mzml = ionic::read("run.ion")?;
    ionic::write("out.ion", &mzml, &WriteOptions::default())?;
    Ok(())
}
```

### Streaming writer

The metadata passed to `create`/`to` carries run-level information; spectra and chromatograms
come from `write_spectrum`/`write_chromatogram` (or `write_stream`), not from the metadata
argument. `finish` is required — `Drop` does not finish the file.

```rust
use ionic::{IonReader, IonWriter, ReadOptions, WriteOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut reader = IonReader::open("run.ion", &ReadOptions::default())?;
    let metadata = reader.metadata()?;

    let mut writer = IonWriter::create("out.ion", &metadata, &WriteOptions::default())?;
    for index in 0..reader.spectrum_count() as usize {
        writer.write_spectrum(&reader.spectrum(index)?)?;
    }
    writer.finish()?;
    Ok(())
}
```

`WriteOptions::default()`:

| Field | Default |
|---|---|
| `compression_level` | `12` (0 = off, 1..=22 zstd) |
| `force_f32` | `false` |
| `block_size` | `1024 * 1024` (1 MiB target uncompressed block) |
| `parallel` | `true` |
| `section_storage` | `SectionStorage::Disk` |
| `mz_window` | `20.0` |

### Convert

```rust
use ionic::ConvertOptions;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    ionic::convert_file("run.mzML", "run.ion", &ConvertOptions::default())?;

    let ion_bytes = std::fs::read("run.ion")?;
    let mzml_bytes = ionic::convert(&ion_bytes, &ConvertOptions::default())?;
    assert!(mzml_bytes.starts_with(b"<?xml"));
    Ok(())
}
```

`ConvertOptions::default()`:

| Field | Default |
|---|---|
| `kind` | `ConvertKind::Auto` (sniffs the extension, then the file signature) |
| `read` | `ReadOptions::default()` |
| `write` | `WriteOptions::default()` |

### Partial reads — ionic::source

`byte_ranges` and `eic_byte_ranges` turn a query into the exact byte ranges a remote source
would need to fetch, without reading them; that is what lets a viewer such as
[ion-beam](https://github.com/phenological/ion-beam) show only the bytes it downloaded.

```rust
use ionic::{IonReader, Range, ReadOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut reader = IonReader::open("run.ion", &ReadOptions::default())?;
    let ranges = reader.byte_ranges(0, Range { from: 200.0, to: 400.0 })?;
    for range in ranges {
        println!("fetch bytes {}..{}", range.offset, range.offset + range.length);
    }
    Ok(())
}
```

## How the format works 

A fixed 1024-byte header stores the byte offset of every section. A reader uses these offsets to go directly to the index, then decompresses only the blocks a query needs. [spec/README.MD](spec/README.MD) specifies the design; [spec/v1.md](spec/v1.md) gives the exact byte layout.

## Portability

The library depends only on standard byte operations: no HDF5, no storage engine, nothing
to install alongside a file. Compression uses the pure-Rust [cosmoz](https://crates.io/crates/cosmoz)
on every target, so native and `wasm32-unknown-unknown` share the same zstd code. The wasm build
drops `rayon` and `memmap2`, and browser conversions use compression level 1. That is what
[ion-beam](https://github.com/phenological/ion-beam) runs on.

## Related projects

- **[ion-beam](https://github.com/phenological/ion-beam)**: a browser viewer for `.ion`
  files that downloads only the bytes it needs, with an inspector showing exactly which
  regions of the file were fetched. [Try it live](https://phenological.github.io/ion-beam/).
- **[Quant·ion](https://github.com/phenological/quantion)**: the Rust processing toolkit
  (peak picking, baselines, noise, untargeted feature detection) with Python, R and
  JavaScript wrappers.
- **[ion-files](https://github.com/phenological/ion-files)**: a small public collection of
  demo `.ion` files.

## Citing

If you use Ionic, Quant·ion or ion-beam, please cite:

> Reading only what you need: a dependency-free, streamable format and cross-language toolkit for scalable LC-MS feature detection. Preprint, 2026. DOI: [10.XXXXX/XXXXXX](https://doi.org/10.XXXXX/XXXXXX)

```bibtex
@article{ionic2026,
  title   = {Reading only what you need: a dependency-free, streamable format and cross-language toolkit for scalable LC-MS feature detection},
  author  = {TBD},
  year    = {2026},
  journal = {TBD},
  doi     = {10.XXXXX/XXXXXX}
}
```

## License

[MIT](./LICENSE)
