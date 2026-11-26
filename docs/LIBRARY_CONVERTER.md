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
| KiCad footprint library (folder) | `pretty` | `library` |
| KiCad library tables | `sym-lib-table`, `fp-lib-table` | `library` |

A library table has no extension, so its **full file name** is the entry in
`extensions`. The contract schema's extension pattern is `^[a-z0-9_]+$`, which
does not admit the hyphens in those two names; they are the only entries that
deviate from the schema. (A schema revision should allow `-`, or give tables a
separate field.) The test `describe_matches_the_schema_but_for_the_two_table_names`
pins exactly this deviation.

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

Current state: the read phase runs (every input is hashed into the report's
`sources`; symbol libraries are read by `kicad_parser::symlib`, which fills
`counts.source.symbols` and the report items, see `KICAD_SYMBOL_FORMAT.md`;
an input that does not parse is an `error` item, not a failed run), the report is written into `--output`, and the run then exits 1 with
`error: "library output not implemented yet"`. The Signex writers land in later
units; the id check of `--update` comes with them.

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
