# CLI test fixtures

- `library-converter-v1.json`: the library converter contract's JSON Schema,
  copied unchanged from the Signex repository, `docs/schemas/library-converter-v1.json`
  at signex 440d98329 (a public document; whether this copy may live here is for the maintainer to confirm). The tests validate the converter's
  output against it; refresh the copy when the contract's minor version moves.
- `legacy-help.txt`, `legacy-v7-summary.json`: the board conversion's `--help`
  and `--json` output, captured from this repository's last commit before the
  `describe` / `library` subcommands (696d6b2). `<OUT>` stands for the output
  directory. They pin that the command without a subcommand did not change.
