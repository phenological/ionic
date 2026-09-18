use std::{fs::File, io::IsTerminal, path::Path, sync::OnceLock};

use cosmoz::{DecompressOptions, Decoder, decompress_into};

use ionic::ion::{
    CODEC_NONE, CODEC_ZSTD, FILE_SIGNATURE, FILE_TRAILER, HEADER_SIZE, get_version_from_header,
    is_supported,
};

static COLOR_ENABLED: OnceLock<bool> = OnceLock::new();

fn color_enabled() -> bool {
    *COLOR_ENABLED.get_or_init(|| {
        std::env::var_os("NO_COLOR").is_none()
            && std::io::stdout().is_terminal()
            && std::io::stderr().is_terminal()
    })
}

fn ansi(code: &'static str) -> &'static str {
    if color_enabled() { code } else { "" }
}

fn reset() -> &'static str {
    ansi("\x1b[0m")
}

fn green() -> &'static str {
    ansi("\x1b[1;32m")
}

fn red() -> &'static str {
    ansi("\x1b[1;31m")
}

fn bold() -> &'static str {
    ansi("\x1b[1m")
}

fn dim() -> &'static str {
    ansi("\x1b[2m")
}

const SPEC_SUMMARY_ROW: usize = 80;
const INDEX_ENTRY_ROW: usize = 16;
const ARRAY_ADDRESS_ROW: usize = 32;

struct FixedSections {
    spec_summary: Result<Vec<u8>, String>,
    spec_entries: Result<Vec<u8>, String>,
    spec_addresses: Result<Vec<u8>, String>,
    chrom_summary: Result<Vec<u8>, String>,
    chrom_entries: Result<Vec<u8>, String>,
    chrom_addresses: Result<Vec<u8>, String>,
}

fn max_address_index(entries: &[u8]) -> u64 {
    let mut total = 0u64;
    for entry in entries.chunks_exact(INDEX_ENTRY_ROW) {
        let first = u64::from_le_bytes(entry[0..8].try_into().unwrap());
        let count = u64::from_le_bytes(entry[8..16].try_into().unwrap());
        total = total.max(first.saturating_add(count));
    }
    total
}

fn read_fixed_section(
    bytes: &[u8],
    header: &[u8],
    off_at: usize,
    len_at: usize,
    plain_count: u64,
    record: usize,
    compressed: bool,
    decoder: &mut Decoder,
) -> Result<Vec<u8>, String> {
    let off = u64_at(header, off_at) as usize;
    let len = u64_at(header, len_at) as usize;
    let end = off.checked_add(len).ok_or("offset overflow")?;
    let stored = bytes.get(off..end).ok_or("section out of bounds")?;
    if len > 0 && off % 8 != 0 {
        return Err("offset not 8-byte aligned".into());
    }
    if !compressed {
        return Ok(stored.to_vec());
    }
    let plain_len = usize::try_from(plain_count)
        .ok()
        .and_then(|count| count.checked_mul(record))
        .ok_or("section size overflow")?;
    if plain_len == 0 {
        return Ok(Vec::new());
    }
    decompress_section(stored, plain_len, decoder)
}

fn decompress_section(
    stored: &[u8],
    plain_len: usize,
    decoder: &mut Decoder,
) -> Result<Vec<u8>, String> {
    let mut plain = vec![0u8; plain_len];
    let written = decompress_into(stored, &mut plain, &DecompressOptions::default(), decoder)
        .map_err(|e| format!("decompress failed: {e:?}"))?;
    plain.truncate(written);
    Ok(plain)
}

fn read_address_section(
    bytes: &[u8],
    header: &[u8],
    off_at: usize,
    len_at: usize,
    entries: &Result<Vec<u8>, String>,
    compressed: bool,
    decoder: &mut Decoder,
) -> Result<Vec<u8>, String> {
    let entries = entries.as_ref().map_err(|why| why.clone())?;
    let address_count = max_address_index(entries);
    read_fixed_section(
        bytes,
        header,
        off_at,
        len_at,
        address_count,
        ARRAY_ADDRESS_ROW,
        compressed,
        decoder,
    )
}

fn resolve_fixed_sections(bytes: &[u8], header: &[u8], decoder: &mut Decoder) -> FixedSections {
    let compressed = header[11] == CODEC_ZSTD;
    let spec_count = u64_at(header, 256);
    let chrom_count = u64_at(header, 264);

    let spec_summary = read_fixed_section(
        bytes,
        header,
        48,
        56,
        spec_count,
        SPEC_SUMMARY_ROW,
        compressed,
        decoder,
    );
    let spec_entries = read_fixed_section(
        bytes,
        header,
        64,
        72,
        spec_count,
        INDEX_ENTRY_ROW,
        compressed,
        decoder,
    );
    let spec_addresses =
        read_address_section(bytes, header, 80, 88, &spec_entries, compressed, decoder);
    let chrom_summary = read_fixed_section(
        bytes,
        header,
        112,
        120,
        chrom_count,
        SPEC_SUMMARY_ROW,
        compressed,
        decoder,
    );
    let chrom_entries = read_fixed_section(
        bytes,
        header,
        128,
        136,
        chrom_count,
        INDEX_ENTRY_ROW,
        compressed,
        decoder,
    );
    let chrom_addresses = read_address_section(
        bytes,
        header,
        144,
        152,
        &chrom_entries,
        compressed,
        decoder,
    );

    FixedSections {
        spec_summary,
        spec_entries,
        spec_addresses,
        chrom_summary,
        chrom_entries,
        chrom_addresses,
    }
}

fn check_fixed(
    section: &Result<Vec<u8>, String>,
    record: u64,
    count: Option<u64>,
) -> Result<(), String> {
    let plain = section.as_ref().map_err(|why| why.clone())?;
    let len = plain.len() as u64;
    if len % record != 0 {
        return Err(format!("length {len} is not a multiple of {record}"));
    }
    if let Some(expected_count) = count {
        let expected = expected_count.checked_mul(record).ok_or("count overflow")?;
        if len != expected {
            return Err(format!(
                "{} records, expected {expected_count}",
                len / record
            ));
        }
    }
    Ok(())
}

struct Section {
    name: &'static str,
    offset: u64,
    size: u64,
    ok: bool,
}

impl Section {
    fn new(name: &'static str, offset: u64, size: u64) -> Self {
        Self {
            name,
            offset,
            size,
            ok: true,
        }
    }
}

pub(crate) fn check_ion_file(path: &Path) -> Result<(), String> {
    let file = File::open(path).map_err(|e| format!("open failed: {e}"))?;
    let map = unsafe { memmap2::Mmap::map(&file).map_err(|e| format!("mmap failed: {e}"))? };
    let bytes: &[u8] = &map;
    if bytes.len() < HEADER_SIZE {
        return Err(format!(
            "file has {} bytes, expected at least {HEADER_SIZE}",
            bytes.len()
        ));
    }
    let view = HeaderView::new(bytes);
    print_summary(&view);
    println!();
    let (sections_failed, sections_total) = print_sections(&view);
    println!();
    let (integrity_failed, integrity_total) = print_integrity(&view);
    if sections_failed > 0 || integrity_failed > 0 {
        return Err(format!(
            "{sections_failed} of {sections_total} section check(s) failed, \
             {integrity_failed} of {integrity_total} integrity check(s) failed"
        ));
    }
    Ok(())
}

pub(crate) fn ion_file_is_valid(path: &Path) -> bool {
    let Ok(file) = File::open(path) else {
        return false;
    };
    let Ok(map) = (unsafe { memmap2::Mmap::map(&file) }) else {
        return false;
    };
    let bytes: &[u8] = &map;
    if bytes.len() < HEADER_SIZE {
        return false;
    }
    let view = HeaderView::new(bytes);
    section_check_results(&view).iter().all(Result::is_ok)
        && integrity_check_results(&view).iter().all(|(_, ok)| *ok)
}

struct HeaderView<'a> {
    bytes: &'a [u8],
    header: &'a [u8],
    sections: Vec<Section>,
    spec_window_dir: Result<usize, String>,
    chrom_window_dir: Result<usize, String>,
    fixed: FixedSections,
}

impl<'a> HeaderView<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        let header = &bytes[..HEADER_SIZE];
        let sections = build_sections(header, u64_at(header, 400));
        let mut decoder = Decoder::new();
        let spec_window_dir = open_window_directory(bytes, header, 32, 40, 384, &mut decoder);
        let chrom_window_dir = open_window_directory(bytes, header, 96, 104, 392, &mut decoder);
        let fixed = resolve_fixed_sections(bytes, header, &mut decoder);
        Self {
            bytes,
            header,
            sections,
            spec_window_dir,
            chrom_window_dir,
            fixed,
        }
    }

    fn read_header_u32(&self, offset: usize) -> u32 {
        u32_at(self.header, offset)
    }
    fn read_header_u64(&self, offset: usize) -> u64 {
        u64_at(self.header, offset)
    }

    fn signature_ok(&self) -> bool {
        self.header[0..FILE_SIGNATURE.len()] == FILE_SIGNATURE
    }

    fn header_crc_ok(&self) -> bool {
        self.read_header_u32(1020) == crc32fast::hash(&self.header[0..1020])
    }

    fn trailer_ok(&self) -> bool {
        self.bytes.ends_with(&FILE_TRAILER)
    }

    fn file_size_ok(&self) -> bool {
        self.read_header_u64(400) == self.bytes.len() as u64
    }

    fn spec_dir_fits(&self) -> bool {
        self.read_header_u64(240)
            .checked_mul(32)
            .is_some_and(|bytes| bytes <= self.read_header_u64(216))
    }

    fn chrom_dir_fits(&self) -> bool {
        self.read_header_u64(248)
            .checked_mul(32)
            .is_some_and(|bytes| bytes <= self.read_header_u64(232))
    }

    fn sections_in_bounds(&self) -> bool {
        let trailer_start = self.read_header_u64(400).saturating_sub(8);
        self.sections.iter().all(|s| {
            s.size == 0
                || s.offset
                    .checked_add(s.size)
                    .is_some_and(|end| end <= trailer_start)
        })
    }

    fn no_overlaps(&self) -> bool {
        let mut sorted: Vec<&Section> = self.sections.iter().filter(|s| s.size > 0).collect();
        sorted.sort_by_key(|s| s.offset);
        sorted
            .windows(2)
            .all(|w| w[0].offset + w[0].size <= w[1].offset)
    }

    fn all_aligned(&self) -> bool {
        self.sections
            .iter()
            .all(|s| s.size == 0 || s.offset % 8 == 0)
    }

    fn crc_ok(&self, off_at: usize, len_at: usize, stored_at: usize) -> bool {
        let offset = self.read_header_u64(off_at) as usize;
        let len = self.read_header_u64(len_at) as usize;
        let stored = self.read_header_u32(stored_at);
        offset
            .checked_add(len)
            .and_then(|end| self.bytes.get(offset..end))
            .is_some_and(|slice| crc32fast::hash(slice) == stored)
    }

    fn block_dir_crc_ok(
        &self,
        container_off_at: usize,
        container_len_at: usize,
        block_count_at: usize,
        stored_at: usize,
    ) -> bool {
        let container_off = self.read_header_u64(container_off_at);
        let container_len = self.read_header_u64(container_len_at);
        let block_count = self.read_header_u64(block_count_at);
        let stored = self.read_header_u32(stored_at);
        (|| -> Option<bool> {
            let dir_bytes = block_count.checked_mul(32)?;
            if dir_bytes > container_len {
                return Some(false);
            }
            let dir_off = container_off.checked_add(container_len.checked_sub(dir_bytes)?)?;
            let start = usize::try_from(dir_off).ok()?;
            let end = usize::try_from(dir_off.checked_add(dir_bytes)?).ok()?;
            Some(
                self.bytes
                    .get(start..end)
                    .is_some_and(|s| crc32fast::hash(s) == stored),
            )
        })()
        .unwrap_or(false)
    }

    fn axis_maxes(&self) -> AxisMaxes {
        let mut max_rt: f64 = 0.0;
        let mut max_x: u32 = 0;
        let mut max_y: u32 = 0;
        let mut max_z: u32 = 0;

        if let Ok(buf) = &self.fixed.spec_summary {
            for row in buf.chunks_exact(SPEC_SUMMARY_ROW) {
                let rt = f64::from_le_bytes(row[0..8].try_into().unwrap());
                let x = u32::from_le_bytes(row[42..46].try_into().unwrap());
                let y = u32::from_le_bytes(row[46..50].try_into().unwrap());
                let z = u32::from_le_bytes(row[50..54].try_into().unwrap());
                if rt.is_finite() && rt > max_rt {
                    max_rt = rt;
                }
                if x > max_x {
                    max_x = x;
                }
                if y > max_y {
                    max_y = y;
                }
                if z > max_z {
                    max_z = z;
                }
            }
        }

        let max_mz = match &self.spec_window_dir {
            Ok(top_window) => (*top_window as f64) * (self.read_header_u32(24) as f64),
            Err(_) => 0.0,
        };

        AxisMaxes {
            max_rt,
            max_mz,
            max_x,
            max_y,
            max_z,
        }
    }
}

struct AxisMaxes {
    max_rt: f64,
    max_mz: f64,
    max_x: u32,
    max_y: u32,
    max_z: u32,
}

fn open_window_directory(
    bytes: &[u8],
    header: &[u8],
    off_at: usize,
    len_at: usize,
    plain_len_at: usize,
    decoder: &mut Decoder,
) -> Result<usize, String> {
    let off = u64_at(header, off_at) as usize;
    let len = u64_at(header, len_at) as usize;
    if len == 0 {
        return Ok(0);
    }
    let end = off.checked_add(len).ok_or("offset overflow")?;
    let raw = bytes.get(off..end).ok_or("section out of bounds")?;
    let plain = match header[11] {
        CODEC_NONE => raw.to_vec(),
        CODEC_ZSTD => {
            let plain_len = u64_at(header, plain_len_at) as usize;
            decompress_section(raw, plain_len, decoder)?
        }
        _ => return Err("unknown codec".into()),
    };
    if plain.len() < 8 {
        return Err("directory header too short".into());
    }
    let window_count = u32_at(&plain, 0) as usize;
    let entry_count = u32_at(&plain, 4) as usize;
    if window_count == 0 {
        return Err("window_count is zero".into());
    }
    let expected = 8 + (window_count + 1) * 4 + entry_count * 4 * 3;
    if plain.len() != expected {
        return Err(format!(
            "length {} does not match window_count {window_count} and entry_count {entry_count}",
            plain.len()
        ));
    }
    let mut top_window = 0usize;
    for window in 0..window_count {
        let start = u32_at(&plain, 8 + window * 4);
        let next = u32_at(&plain, 8 + (window + 1) * 4);
        if next > start {
            top_window = window + 1;
        }
    }
    Ok(top_window)
}

fn build_sections(header: &[u8], total_file_size: u64) -> Vec<Section> {
    let mut sections = vec![
        Section::new(
            "A0 — spectrum m/z-window directory",
            u64_at(header, 32),
            u64_at(header, 40),
        ),
        Section::new(
            "A1 — spectrum fast-filter summary",
            u64_at(header, 48),
            u64_at(header, 56),
        ),
        Section::new(
            "A2 — spectrum array index",
            u64_at(header, 64),
            u64_at(header, 72),
        ),
        Section::new(
            "A3 — spectrum array address table",
            u64_at(header, 80),
            u64_at(header, 88),
        ),
        Section::new(
            "B0 — chromatogram m/z-window directory",
            u64_at(header, 96),
            u64_at(header, 104),
        ),
        Section::new(
            "B1 — chromatogram fast-filter summary",
            u64_at(header, 112),
            u64_at(header, 120),
        ),
        Section::new(
            "B2 — chromatogram array index",
            u64_at(header, 128),
            u64_at(header, 136),
        ),
        Section::new(
            "B3 — chromatogram array address table",
            u64_at(header, 144),
            u64_at(header, 152),
        ),
        Section::new("spec_meta", u64_at(header, 160), u64_at(header, 168)),
        Section::new("chrom_meta", u64_at(header, 176), u64_at(header, 184)),
        Section::new("global_meta", u64_at(header, 192), u64_at(header, 200)),
        Section::new("spec_container", u64_at(header, 208), u64_at(header, 216)),
        Section::new("chrom_container", u64_at(header, 224), u64_at(header, 232)),
    ];

    let trailer_start = total_file_size.saturating_sub(8);
    for s in &mut sections {
        s.ok = s
            .offset
            .checked_add(s.size)
            .is_some_and(|end| end <= trailer_start)
            && (s.size == 0 || s.offset % 8 == 0);
    }

    let mut order: Vec<usize> = (0..sections.len())
        .filter(|&i| sections[i].size > 0)
        .collect();
    order.sort_by_key(|&i| sections[i].offset);
    for pair in order.windows(2) {
        let (l, r) = (pair[0], pair[1]);
        let end = sections[l].offset.saturating_add(sections[l].size);
        if end > sections[r].offset {
            sections[l].ok = false;
            sections[r].ok = false;
        }
    }

    sections
}

fn print_summary(view: &HeaderView<'_>) {
    let (bold, reset) = (bold(), reset());
    println!("{bold}File Summary{reset}");
    let sig = bytes_text(&view.header[0..8]);
    field(
        "signature",
        &format!("\"{sig}\""),
        Some(view.signature_ok()),
    );
    let (version_text, version_ok) = match get_version_from_header(view.bytes) {
        Some(v) => (v.to_string(), Some(is_supported(v))),
        None => ("?".to_string(), Some(false)),
    };
    field("format_version", &version_text, version_ok);
    let codec = match view.header[11] {
        0 => "none",
        1 => "zstd",
        _ => "unknown",
    };
    field(
        "codec",
        &format!("{codec}  level {}", view.header[12]),
        None,
    );
    let filter = match view.header[13] {
        0 => "none",
        1 => "shuffle",
        _ => "unknown",
    };
    field("array_filter", filter, None);
    let block_size = view.read_header_u64(16);
    field(
        "block_size",
        &format!(
            "{block_size} bytes  ({:.2} MB)",
            block_size as f64 / (1024.0 * 1024.0)
        ),
        None,
    );
    let target_mz_window = view.read_header_u32(24);
    field("mz_window", &format!("{target_mz_window}"), None);
    field(
        "spectrum_count",
        &view.read_header_u64(256).to_string(),
        None,
    );
    field("chrom_count", &view.read_header_u64(264).to_string(), None);
    let actual = view.bytes.len() as u64;
    field(
        "total_file_size",
        &format!(
            "{actual} bytes  ({:.2} MB)",
            actual as f64 / (1024.0 * 1024.0)
        ),
        Some(view.file_size_ok() && view.trailer_ok()),
    );

    let ax = view.axis_maxes();
    field("max_mz", &format!("{:.6}", ax.max_mz), None);
    field("max_rt", &format!("{:.6}", ax.max_rt), None);
    if ax.max_x > 0 || ax.max_y > 0 || ax.max_z > 0 {
        field("max_x", &ax.max_x.to_string(), None);
        field("max_y", &ax.max_y.to_string(), None);
        field("max_z", &ax.max_z.to_string(), None);
    }
}

fn section_check_results(view: &HeaderView<'_>) -> [Result<(), String>; 8] {
    let spec_count = view.read_header_u64(256);
    let chrom_count = view.read_header_u64(264);
    [
        view.spec_window_dir.clone().map(|_| ()),
        check_fixed(
            &view.fixed.spec_summary,
            SPEC_SUMMARY_ROW as u64,
            Some(spec_count),
        ),
        check_fixed(&view.fixed.spec_entries, 16, Some(spec_count)),
        check_fixed(&view.fixed.spec_addresses, 32, None),
        view.chrom_window_dir.clone().map(|_| ()),
        check_fixed(
            &view.fixed.chrom_summary,
            SPEC_SUMMARY_ROW as u64,
            Some(chrom_count),
        ),
        check_fixed(&view.fixed.chrom_entries, 16, Some(chrom_count)),
        check_fixed(&view.fixed.chrom_addresses, 32, None),
    ]
}

fn print_sections(view: &HeaderView<'_>) -> (usize, usize) {
    let (bold, reset, red, dim, green) = (bold(), reset(), red(), dim(), green());
    println!("{bold}Sections{reset}");
    let checks = section_check_results(view);

    let mut failed = 0;
    for (section, result) in view.sections[..checks.len()].iter().zip(checks.iter()) {
        if let Err(why) = result {
            let label = section.name;
            println!("  {red}✗{reset}  {label}  {dim}({why}){reset}");
            failed += 1;
        }
    }
    if failed == 0 {
        println!("  {green}✓{reset}  all {} sections opened", checks.len());
    }
    (failed, checks.len())
}

fn integrity_check_results(view: &HeaderView<'_>) -> [(&'static str, bool); 23] {
    [
        ("1   file signature", view.signature_ok()),
        ("2   header CRC-32", view.header_crc_ok()),
        ("3   file trailer", view.trailer_ok()),
        ("4   file size matches header", view.file_size_ok()),
        ("5   spec block dir fits container", view.spec_dir_fits()),
        ("6   chrom block dir fits container", view.chrom_dir_fits()),
        ("7   all sections within bounds", view.sections_in_bounds()),
        ("8   no section overlaps", view.no_overlaps()),
        ("9   all offsets 8-byte aligned", view.all_aligned()),
        (
            "10  A0 spec_window_directory CRC-32",
            view.crc_ok(32, 40, 968),
        ),
        ("11  A1 spec_summary CRC-32", view.crc_ok(48, 56, 972)),
        ("12  A2 spec_entries CRC-32", view.crc_ok(64, 72, 976)),
        (
            "13  A3 spec_array_addresses CRC-32",
            view.crc_ok(80, 88, 980),
        ),
        (
            "14  B0 chrom_window_directory CRC-32",
            view.crc_ok(96, 104, 984),
        ),
        ("15  B1 chrom_summary CRC-32", view.crc_ok(112, 120, 988)),
        ("16  B2 chrom_entries CRC-32", view.crc_ok(128, 136, 992)),
        (
            "17  B3 chrom_array_addresses CRC-32",
            view.crc_ok(144, 152, 996),
        ),
        (
            "18  spec block directory CRC-32",
            view.block_dir_crc_ok(208, 216, 240, 1000),
        ),
        (
            "19  chrom block directory CRC-32",
            view.block_dir_crc_ok(224, 232, 248, 1004),
        ),
        ("20  C spec_meta CRC-32", view.crc_ok(160, 168, 1008)),
        ("21  D chrom_meta CRC-32", view.crc_ok(176, 184, 1012)),
        ("22  E global_meta CRC-32", view.crc_ok(192, 200, 1016)),
        (
            "23  format version supported",
            get_version_from_header(view.bytes)
                .map(is_supported)
                .unwrap_or(false),
        ),
    ]
}

fn print_integrity(view: &HeaderView<'_>) -> (usize, usize) {
    let (bold, reset, red, green) = (bold(), reset(), red(), green());
    println!("{bold}Integrity Checks{reset}");
    let checks = integrity_check_results(view);
    let mut failed = 0;
    for (label, ok) in checks {
        if !ok {
            println!("  {red}✗{reset}  {label}");
            failed += 1;
        }
    }
    let passed = checks.len() - failed;
    println!();
    let color = if failed == 0 { green } else { red };
    println!(
        "{color}{bold}{passed}/{} checks passed{reset}",
        checks.len()
    );
    (failed, checks.len())
}

fn field(name: &str, value: &str, status: Option<bool>) {
    let (dim, reset, green, red) = (dim(), reset(), green(), red());
    let indicator = match status {
        None => String::new(),
        Some(true) => format!("  {green}✓{reset}"),
        Some(false) => format!("  {red}✗{reset}"),
    };
    println!("  {dim}{name:<20}{reset}  {value}{indicator}");
}

fn bytes_text(bytes: &[u8]) -> String {
    let mut text = String::new();
    for &byte in bytes {
        match byte {
            0 => text.push_str("\\0"),
            b' '..=b'~' => text.push(byte as char),
            _ => text.push_str(&format!("\\x{byte:02x}")),
        }
    }
    text
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    let mut out = [0; 4];
    out.copy_from_slice(&bytes[offset..offset + 4]);
    u32::from_le_bytes(out)
}

fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    let mut out = [0; 8];
    out.copy_from_slice(&bytes[offset..offset + 8]);
    u64::from_le_bytes(out)
}
