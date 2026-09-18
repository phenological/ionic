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

### Convert

`ionic::convert` takes a path or a byte buffer. With `output` set it writes the file and returns `None`. With `output` empty it returns the result as `Some(bytes)`. `kind` defaults to `ConvertKind::Auto`, which reads the direction from the `.mzML` or `.ion` extension, and from the file signature for a buffer or a file without one of those extensions.

```rust
use ionic::{ConvertKind, ConvertOptions, WriteOptions};

ionic::convert(Path::new("run.mzML"), ConvertOptions { output: Some("run.ion".into()), ..Default::default() })?;
ionic::convert(Path::new("run.ion"), ConvertOptions { output: Some("run.mzML".into()), ..Default::default() })?;

let ion_bytes = ionic::convert(Path::new("run.mzML"), ConvertOptions::default())?.unwrap();
let mzml_bytes = ionic::convert(&ion_bytes, ConvertOptions::default())?.unwrap();

ionic::convert(
    &mzml_bytes,
    ConvertOptions {
        output: Some("run.ion".into()),
        kind: ConvertKind::MzmlToIon,
        write: WriteOptions { compression_level: 0, ..Default::default() },
        ..Default::default()
    },
)?;
```

### Read an .ion file

```rust
use ionic::{ArrayKind, IonReader, ReadOptions};

let mut reader = IonReader::open_file(Path::new("run.ion"), ReadOptions::default())?;
let mz = reader.get_spectrum_array(0, ArrayKind::Mz)?;
let intensity = reader.get_spectrum_array(0, ArrayKind::Intensity)?;
```

### Reading options — ReadOptions

```rust
pub struct ReadOptions {
    pub max_cached_bytes: usize,                 // decoded-block cache cap; default 256 MiB
    pub verify_checksums: bool,                  // check CRCs + layout on open; default true
    pub parallel: bool,                          // decode blocks in parallel; default true
    pub decompression_limit: DecompressionLimit, // zip-bomb guard; type from ionic::ion::
}
```

### Write an .ion file

Memory

```rust
use ionic::{IonWriter, MemoryReader, MzML, Spectrum, WriteOptions};

let mzml = MzML::from_spectra(vec![Spectrum::new("scan=1", vec![100.0, 200.0], vec![10.0, 20.0])]);
let mut source = MemoryReader::new(mzml);
let mut out: Vec<u8> = Vec::new();
let mut writer = IonWriter::create(&mut out, WriteOptions::default())?;
writer.write_stream(&mut source)?;
```

File path

```rust
use ionic::{FileWriter, IonWriter, MemoryReader, MzML, Spectrum, WriteOptions};

let mzml = MzML::from_spectra(vec![Spectrum::new("scan=1", vec![100.0, 200.0], vec![10.0, 20.0])]);
let mut source = MemoryReader::new(mzml);
let mut output = FileWriter::open_path(Path::new("out.ion"))?; // or FileWriter::open("out.ion")
let mut writer = IonWriter::create(&mut output, WriteOptions::default())?;
writer.write_stream(&mut source)?;
output.flush()?;
```

### WriteOptions

```rust
pub struct WriteOptions {
    pub compression_level: u8,           // 0 = off, 1..=22 zstd; default 12
    pub force_f32: bool,                 // narrow f64 arrays to f32 (lossy); default false
    pub block_size: usize,               // target uncompressed block bytes; default 1 MiB
    pub parallel: bool,                  // default true
    pub section_storage: SectionStorage, // Memory or Disk; default Disk
    pub mz_window: f64,                  // m/z window width for range-read indexing; default 100.0
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
