# signex-kicad-import

One-way converter from KiCad boards to Signex's current native files: for
each board, `<name>.snxpcb` (format `snxpcb/1.4`) and a `<name>.snxprj`
that carries the project's net classes.

**Licensed GPL-3.0-or-later** because it implements KiCad's file
format. The Signex writer in `crates/snx-writer` is this repo's own
code, written from the format notes in `docs/SNX_FORMAT_NOTES.md`; it
does not copy Signex source. This companion tool is distributed
independently from [Signex](https://github.com/alplabai/signex).

## Usage

```bash
signex-kicad-import path/to/board.kicad_pcb            # writes board.snxpcb + board.snxprj
signex-kicad-import path/to/project.kicad_pro -o out/  # every .kicad_pcb in the project folder
signex-kicad-import --json --force board.kicad_pcb     # JSON summary, replace existing output
```

Existing output files are not replaced unless `--force` is given. Every
board is converted and checked before anything is written, so a board
that fails leaves no output. A board's two files are written together:
a `.snxpcb` never appears without its `.snxprj`, and a failed write
leaves no temporary file behind.

The summary lists what could not be carried over exactly (by kind and
count) and a self-check: how many track ends land on a pad of their own
net, and how many only on another net's pad, once Signex places the pads.

Schematic and symbol conversion is not part of this release.

## What the board conversion covers

- Layers mapped to Signex layer tokens (`TopCopper`, `Inner<n>Copper`,
  `BottomCopper`, `TopSilkscreen`, `EdgeCuts`, ...), the copper stack
  top to bottom with plane layers marked, and the board's own copper
  layer names kept as display names.
- Footprints with their pads (circle, rect, oval, roundrect, custom and
  trapezoid pads as custom outlines), drills, nets, drawings and fields.
  The designator and value texts become the Reference and Value fields.
  A bottom-side footprint is written library side up (the source's flip
  negates local y and turns the part a half turn); Signex mirrors it back.
- Tracks, copper arcs, through, blind and micro vias.
- Zones as polygon pours with their cut-outs as holes, rule areas as
  keepouts, filled copper polygons as solid copper; one zone per copper
  layer. A hatched zone and a zone that leaves pads unconnected are
  reported.
- The board outline: the largest closed edge ring. Other edge drawings
  (cut-outs) stay drawings on the edge layer.
- The board origin (the drill and place origin, else the grid origin).
- Net classes and board rules from the `.kicad_pro` (or the classes
  stored in older board files) into the `.snxprj`.

Item identities are kept; a missing or repeated one is replaced by a
fresh one drawn deterministically from the input, so the same input
always gives the same files.

## Tests

`crates/kicad-parser/tests/real_boards.rs` converts four real boards, one
per source format generation (6 to 9); they are listed with their
licences in `crates/kicad-parser/tests/fixtures/README.md`.

## Install

From source:
```bash
cargo install --path crates/cli
```

## License

GPL-3.0-or-later. See `LICENSE`.
