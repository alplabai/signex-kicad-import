# Library converter commands

`signex-kicad-import` follows the Signex library converter contract (v1.0,
`docs/LIBRARY_CONVERTER_CONTRACT.md` in the Signex repository). Without a
subcommand it is still the board converter described in the README; its
output did not change.

## `describe --json`

Prints the contract's `describe` document and exits 0:

| Input | Extension entry | Kind |
|---|---|---|
| KiCad symbol library (one file, many symbols) | `kicad_sym` | `library` |
| KiCad symbol directory (one symbol per file) | `kicad_symdir` | `library` |
| KiCad footprint library (folder), or one footprint file | `pretty`, `kicad_mod` | `library` |
| KiCad library tables | `sym-lib-table`, `fp-lib-table` | `library` |

A library table has no extension, so its **full file name** is the entry in
`extensions` (the contract allows fixed names of source files).

## `library <input>... --output <dir>`

All contract options are parsed and checked: `--name`, `--library-id` (a
UUID), `--db`, `--search-path` (repeatable, folders), `--map` (a file),
`--models include|skip`, `--models-dir` (a folder), `--dry-run`, `--update`,
`--json`, `--progress` (implies `--json`). Unknown options, a missing
`--output`, a bad value, a missing or unsupported input, and a non-empty
`--output` without `--update` (or `--update` on a missing one) fail with exit
1 and, with `--json`/`--progress`, a summary with `ok: false`. Such a run
writes nothing. (clap's own exit code 2 for usage errors is mapped to 1 here,
because 2 means `partial` in this contract.)

Inputs are classified by name and shape: `*.kicad_sym` (file),
`*.kicad_symdir` (folder), `*.pretty` (folder), `sym-lib-table` and
`fp-lib-table` (files).

Current state: the read phase hashes every input and reads symbol libraries
(`kicad_parser::symlib`, see `KICAD_SYMBOL_FORMAT.md`) and footprints
(`.pretty` folders and single `.kicad_mod` files, `kicad_parser::footprint_lib`);
an input that does not parse is an `error` item, not a failed run. The convert
phase maps them to Signex symbols and footprints (`LIBRARY_MAPPING.md`) and the
write phase stages and moves `symbols/*.snxsym`, `footprints/<uuid>.snxfpt` and
a `.snxlib` manifest into `--output`. Exit 0 when only `info` / `approximated`
items arose, 2 when something was dropped or an item failed, 1 when nothing
usable could be read. `--dry-run` maps everything and writes only the report.
`--update` needs the report of a library with the same id and overwrites in
place. Library tables, 3D models and part rows are not converted yet.

### Streams

- `--progress`: NDJSON on stdout, one flushed line per event (`phase`,
  `progress`, last `summary`). A heartbeat thread repeats the last
  `progress` event after 5 s of silence (the contract allows 10 s), and is
  stopped before the summary so the summary is always the last line.
- `--json`: only the summary, as the last (only) stdout line.
- Diagnostics (`error: ...`) go to stderr.
- Neither flag: a short human line on stdout.

### Report and exit codes

`import-report.json` (schema `#/$defs/report`) and `import-report.md` are
written into `--output`, also by a run that fails after reading its sources.
Items carry `kind`, `source`, `id`, `code`, `severity`, `message`, `detail`.
Exit 0: only `info` / `approximated` items; 2: some item `dropped` or `error`;
1: failed. `warnings` in the summary aggregates the non-`info` items by
message. `started` is the run's UTC time; `sources[].sha256` is the file's
digest, or for a folder the digest of every file in it (relative path, length,
bytes; sorted by path).

## Report codes

The board conversion counts free-text messages. For the structured report each
message maps, without changing its text (so the legacy `--json` summary is
byte-identical), to a stable `<area>.<what>` code and a severity; see
`REPORT_CODES.md`. A message the catalogue does not know is
`board.unclassified` / `approximated` with the text in `detail`; a test fails
when a message written in the sources is missing from the catalogue.

## Failure and cleanup

A panic is a failed run: exit 1 and a summary `internal error: ...`. Library
files are written to a staging folder (`.<output>.tmp-import` beside
`--output`) and moved in only on success, so a failed run leaves none, and a
library that existed before `--update` stays as it was. `import-report.*` stays
once the sources were read. A failed summary has `library: null`.
