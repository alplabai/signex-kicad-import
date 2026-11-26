# CLI test fixtures

- `library-converter-v1.json`: the library converter contract's JSON Schema,
  copied unchanged from the Signex repository, `docs/schemas/library-converter-v1.json`
  at signex 9a3fdc1cb. The schema file alone is licensed MIT OR Apache-2.0
  (SPDX identifier in its `$comment`, kept in the copy); the rest of the Signex
  repository is not, which is why the library-format fixtures are read through
  `SIGNEX_FORMAT_FIXTURES` and not copied. Refresh the copy when the contract's
  minor version moves.
- `legacy-help.txt`, `legacy-v7-summary.json` (the help text was regenerated when `.kicad_sym` input was added; the board summary is unchanged): the board conversion's `--help`
  and `--json` output, captured from this repository's last commit before the
  `describe` / `library` subcommands (696d6b2). `<OUT>` stands for the output
  directory. They pin that the command without a subcommand did not change.
