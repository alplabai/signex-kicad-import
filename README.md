# signex-kicad-import

One-way converter from KiCad files (`.kicad_sch`, `.kicad_pcb`,
`.kicad_sym`) to Signex's native formats (`.snxsch`, `.snxpcb`,
`.snxsym`).

**Licensed GPL-3.0-or-later** because it implements KiCad's file
format with structure derived from KiCad's GPL-3.0 source. This
companion tool is distributed independently from
[Signex Community Edition](https://github.com/alplabai/signex)
which is Apache-2.0.

## Usage

```bash
signex-kicad-import path/to/project.kicad_pro
```

Produces `project.snxprj` + `.snxsch` / `.snxpcb` siblings. Open
the resulting `.snxprj` in Signex Community.

## Install

Pre-built binaries on the [Releases page](https://github.com/alplabai/signex-kicad-import/releases)
for Linux / Windows / macOS.

From source:
```bash
cargo install --path crates/cli
```

## Why two repos?

Signex Community is Apache-2.0. KiCad's source is GPL-3.0, and
file-format implementations derived from it are subject to KiCad's
reciprocal terms. To preserve a clean Apache codebase for Signex,
KiCad I/O is shipped here as an optional GPL-3.0 companion. Apache
consumers of Signex Community see no GPL aggregation; users who
want to migrate KiCad projects download both tools.

## License

GPL-3.0-or-later. See `LICENSE`.
