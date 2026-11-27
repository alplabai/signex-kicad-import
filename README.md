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
signex-kicad-import Device.kicad_sym -o out/                   # one symbol library, written as Device.snxsym
```

Existing output files are not replaced unless `--force` is given. Every
board is converted and checked before anything is written, so a board
that fails leaves no output. A board's two files are written together:
a `.snxpcb` never appears without its `.snxprj`, and a failed write
leaves no temporary file behind.

The summary lists what could not be carried over exactly (by kind and
count) and a self-check: how many track ends land on a pad of their own
net, and how many only on another net's pad, once Signex places the pads.

A `.kicad_sym` given without a subcommand is converted to one `<name>.snxsym`
(derived symbols, `extends`, are written as symbols of their own; what could not
be carried over is listed in the summary by message and count, like a board).
Schematic conversion is not part of this release.

For Signex's library converter contract, `signex-kicad-import describe --json`
and `signex-kicad-import library <inputs>... --output <dir>` (KiCad symbol
libraries and tables) are described in `docs/LIBRARY_CONVERTER.md`; the
`library` command converts KiCad symbol libraries and footprints into a Signex
library directory (`docs/LIBRARY_MAPPING.md`: symbols, footprints and part rows,
from files, folders or library tables; 3D STEP models are embedded).

## What the board conversion covers

- Layers mapped to Signex layer tokens (`TopCopper`, `Inner<n>Copper`,
  `BottomCopper`, `TopSilkscreen`, `EdgeCuts`, ...), the copper stack
  top to bottom with plane layers marked, and the board's own copper
  layer names kept as display names.
- The physical stackup (copper, dielectric, mask, paste and silkscreen
  rows with thickness, material, permittivity and copper weight) as the
  board's stackup rows.
- Footprints with their pads (circle, rect, oval, roundrect, trapezoid,
  and custom pads with the anchor shape and every filled primitive merged
  into one outline), drills, nets, drawings and fields. The designator
  and value texts become the Reference and Value fields. A bottom-side
  footprint is written library side up (the source's flip negates local
  y and turns the part a half turn); Signex mirrors it back.
- Tracks, copper arcs, through, blind and micro vias. Unfilled copper
  drawings become tracks and copper arcs along their stroke.
- Zones as polygon pours with their cut-outs as holes, rule areas as
  keepouts, filled copper polygons as solid copper; one zone per copper
  layer. Zones inside footprints become board zones. A hatched zone and
  a zone that leaves pads unconnected are reported.
- Arcs inside polygons (zones, custom pads, board and footprint polygons,
  the board edge) become short chords.

Anything the conversion cannot carry (3D models, text boxes, curves,
dimensions, targets, tables, hidden footprint texts, an item whose
position or size does not read) is dropped and reported by kind and
count, never dropped silently. When a file cannot be written, the
temporary files are removed and no board is written.
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

The writers for `.snxsym`, `.snxfpt` and the `.snxlib` manifest are checked
against the Signex library format's conformance fixtures, which belong to the
Signex repository and are not copied here:

```bash
SIGNEX_FORMAT_FIXTURES=/path/to/signex/docs/fixtures/library-format \
  cargo test -p snx-writer --test library_conformance
```

Without the variable those tests print a skip message and pass.

## Install

From source:
```bash
cargo install --path crates/cli
```

## License

GPL-3.0-or-later. See `LICENSE`.
