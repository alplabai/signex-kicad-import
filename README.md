# signex-kicad-import

One-way converter from a KiCad board (`.kicad_pcb`, or the `.kicad_pro`
beside it) to Signex's current native files: `<name>.snxpcb` (format
`snxpcb/1.4`) and a `<name>.snxprj` that carries the project's net
classes.

**Licensed GPL-3.0-or-later** because it implements KiCad's file
format. The Signex writer in `crates/snx-writer` is this repo's own
code, written from the public description of the Signex formats; it
does not copy Signex source. This companion tool is distributed
independently from [Signex](https://github.com/alplabai/signex).

## Usage

```bash
signex-kicad-import path/to/board.kicad_pcb            # writes board.snxpcb + board.snxprj
signex-kicad-import path/to/board.kicad_pro -o out/    # same, into out/
signex-kicad-import --json --force board.kicad_pcb     # JSON summary, replace existing output
```

Existing output files are not replaced unless `--force` is given. The
summary lists what could not be carried over exactly (by kind and count)
and a self-check: how many track ends land on a pad of their own net,
and how many on another net's pad, once Signex places the pads.

Schematic and symbol conversion is not part of this release.

## What the board conversion covers

- Layers mapped to Signex layer tokens (`TopCopper`, `Inner<n>Copper`,
  `BottomCopper`, `TopSilkscreen`, `EdgeCuts`, ...), the copper stack
  top to bottom with plane layers marked.
- Footprints with their pads (circle, rect, oval, roundrect, custom and
  trapezoid pads as custom outlines), drills, nets, drawings and fields.
  Bottom-side footprints are written library-side up; Signex mirrors them.
- Tracks, copper arcs, through, blind and micro vias.
- Zones as polygon pours, rule areas as keepouts, filled copper polygons
  as solid copper; one zone per copper layer.
- The board outline: the largest closed edge ring. Other edge drawings
  (cut-outs) stay drawings on the edge layer.
- Net classes and board rules from the `.kicad_pro` (or the classes
  stored in older board files) into the `.snxprj`.

Item identities are kept; a missing or repeated one is replaced by a
fresh one drawn deterministically from the input, so the same input
always gives the same files.

## Install

From source:
```bash
cargo install --path crates/cli
```

## License

GPL-3.0-or-later. See `LICENSE`.
