# IONIC — overview

IONIC stores one mass-spectrometry run in one binary file: spectra, chromatograms, and their metadata. It is built to open fast and to read very large files without loading them whole.

This page explains the idea. For the exact byte layout, read the spec for the current version: [`v1.md`](v1.md).

## One file, found through the header

The file opens with a fixed 1024-byte header. The header holds the global settings and, for every section, the byte offset and length used to find it. A reader trusts the header, not the order of sections on disk. So the parts can be laid out in any order, the file can be written in one pass, and the header is patched in last.

The file starts with the signature `IONIC\0\0\0` and ends with the trailer `\0\0\0CINOI`, and the header stores the total file size. Together these catch a truncated or wrong file at once. The header, each metadata section, every data block, and the block directory that locates them all carry a CRC-32, so a reader can check a part before it trusts it.

## The parts

- **Array blocks**: the heavy data. The numbers (m/z, intensity, time) are packed into typed blocks and compressed. Spectra and chromatograms each get their own container, with a directory of blocks at the end. A reader decompresses only the blocks it needs.
- **A0/A1/A2/A3** (spectra) and **B0/B1/B2/B3** (chromatograms): the tables that address that data.
  - A0/B0 are the window directory: for every m/z window, which spectra store data in it and which array segments hold it. This is the cross-item index an extracted-ion read enters first, so it never has to walk every spectrum. It holds pointers only; A3/B3 stay the single source of truth for block, offset, and type. Chromatograms are not split by m/z, so B0 puts every chromatogram in a single window.
  - A1/B1 hold a short fixed-size summary per item (retention time, base peak, polarity, MS level, and a few more). One 80-byte record per item, so a reader can filter items without decoding any array data.
  - A2/B2 point from an item to its list of arrays.
  - A3/B3 describe each array segment: which block holds it, how many elements it has, its number type, and its filter.
- **C/D/E**: metadata in columns. Every CV parameter is stored by its accession (for example `MS:1000285`, not the name "total ion current"). C is for spectra, D for chromatograms, E for global metadata such as instrument, software, and samples.

## How a read works

Reading one whole item goes `A1 → A2 → A3`:

1. **Filter**: read A1/B1 and keep the items you want. Nothing is decoded yet.
2. **Locate**: for a kept item, read A2/A3 to find its arrays and the blocks that hold them.
3. **Decode**: decompress only those blocks and read the arrays.
4. **Resolve metadata**: read the item's parameters from the columns in C/D. This is the columnar model, explained next.

Reading one m/z range across many items goes `A0 → A1 → A3` instead: enter the window directory, take the entries for the windows that touch the range, read `A1[spectrum_index]` for retention time, and follow the A3 rows those entries name.

A spectrum's arrays are cut into ordered **segments by m/z window**. Segments in the same window are routed to shared blocks across spectra, so an extracted-ion read opens only the blocks whose window overlaps the requested m/z range. **A0** holds, per window, the spectra that store data there and the A3 rows of their m/z and intensity segments, so a reader decodes only the windows that touch the range and skips the rest. The m/z window read is a convenience wrapper over a generic accession-based window read. Small arrays are left whole. The exact bytes are in [`v1.md`](v1.md).

Retention times and m/z values must be monotonic: equal or increasing values are accepted, and a lower value than the previous one is rejected. This keeps the entries in each window in rt order, so retention-time ranges can be found by binary search.

## Metadata is columnar

Instead of keeping each parameter as its own small object, IONIC lays every field down its own column (one long array per field) and stores the columns side by side. Columns hold one kind of value, so they pack and compress far better than mixed objects, and a reader can load only the columns it needs.

Every CV parameter becomes one **row**. All rows share the same columns: find the rows for an item, read across those columns, and you have everything the item carries. The exact column names, types, and order are in [`v1.md`](v1.md). C holds these columns for spectra, D for chromatograms, E for global metadata.

C and D are split into fixed groups of items, each group compressed on its own with its own value pools, so reading one item's metadata decompresses one group rather than the whole section. E is a single section: it does not grow with the number of items.

## A1/B1 are only a cache

Every value in A1/B1 also lives in C/D under its accession, so A1/B1 can be rebuilt from C/D at any time. They exist only to make filtering fast, which is why they are the one part of version 1 allowed to grow: each record has a reserved tail that may gain new optional fields. Readers ignore tail slots they do not know, and writers zero any tail bytes they do not set. Once a slot is given an offset it keeps that offset, type, unit, and meaning forever. The rules are in [`v1.md`](v1.md).

Everything else is fixed. Changing any other layout, or the header's reserved area, is a real format change: a new `format_version` and a new spec file.

