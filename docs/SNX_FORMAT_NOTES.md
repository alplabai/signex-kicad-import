# Signex board format notes

What `crates/snx-writer` relies on when it writes `.snxpcb` and `.snxprj`
files: the format's rules as a Signex reader applies them, in plain
words. The writer, the cell encoding (`tsv_cell`), the custom-pad outline
check (`ring.rs`) and the placement self-check (`placement_check.rs`) in
this repository were written from these notes and from the behaviour they
describe; no Signex source code was copied into this repository.

The library files (`.snxsym`, `.snxfpt`, `.snxlib`) are written from the
separate public library format; what the converter does to a KiCad library is in
`LIBRARY_MAPPING.md`, the facts it relies on for KiCad symbols in
`KICAD_SYMBOL_FORMAT.md`. This file covers the board files only.

## Files

- A `.snxpcb` is TOML. It starts with `format = "snxpcb/1.4"`, `pcb_id`,
  `version`, `generator` and `thickness`, before any table.
- Bulk rows (footprints, pads, tracks, vias) are TSV tables held in a
  `content` string under `[footprints]`, `[pads]`, `[tracks]` and `[vias]`.
  The first line names the columns. Lengths in rows are whole nanometres.
- Other data is TOML: `[stackup]`, `[nets]`, `[[zones]]`, `[[free.arcs]]`,
  `[board_options.origin]` and the `[extras]` tables. Lengths there are
  millimetres, except under `[free]` and `[board_options]`, which are
  whole nanometres too.
- A `.snxprj` is JSON naming the board (`pcb_file`) and carrying the net
  classes (`net_classes`).

## Cells

A reader splits a row on runs of whitespace and reads a lone `-` as an
empty cell. So a value that is empty, is `-`, or holds whitespace, a
double quote or a backslash is written between double quotes. Between the
quotes a double quote is written twice, and a backslash, line feed,
carriage return or tab is written as `\\`, `\n`, `\r` or `\t`. Any other
value is written as it is. The TSV body then sits in a TOML multi-line
basic string, so its backslashes and quotes are escaped once more for
TOML.

## Layers

Layer names are tokens: `TopCopper`, `Inner1Copper` to `Inner30Copper`,
`BottomCopper`, `TopSilkscreen`, `TopSolderMask`, `TopPaste`, `TopFab`,
`TopCourtyard` (and their `Bottom` forms), `EdgeCuts`, `Margin`, `Notes`,
`Dimensions`, `Eco1`, `Eco2`, `Mechanical1` to `Mechanical32`.
`[stackup] layer_types` gives `signal` or `plane` for copper and an empty
string otherwise; `[stackup.display_names]` maps a token to the name the
board gives that layer.

## Physical stack

The board's physical stack is an optional `[[stackup.rows]]` list, top to
bottom. Each row has `kind` (`signal`, `plane`, `dielectric`,
`solder_mask`, `paste` or `overlay`) and `thickness_mm`; `name`, `layer`
(the copper token of a copper row), `material`, `dielectric_constant`
(Er) and `copper_weight_oz` are left out when empty or 0. The copper rows
name the board's copper layers, top to bottom, each once; every number is
finite and not negative. A board without rows shows a stack Signex
derives from its layers and thickness.

## Footprints and pads

A footprint row carries the footprint's position, its turn in degrees and
its side (`TopCopper` or `BottomCopper`). Pad positions are in the
footprint's own frame, library side up. Signex places a local point by
mirroring x for a bottom-side footprint, then turning it with the plain
rotation matrix (`x cos - y sin`, `x sin + y cos`, y pointing down), then
moving it to the footprint's position. A pad's own turn adds to the
footprint's on top and subtracts from it on the bottom. A pad row keys its
footprint by the footprint's uuid.

## Custom pad outlines

A custom pad's corners are written in the optional `custom_outline` column
as `x,y;x,y;...` in whole nanometres, in the pad's own frame. Signex refuses
an outline that is not a simple polygon on those whole-nanometre corners:
fewer than three corners, a zero-length edge, no area, two edges in a row
that fold back on each other, or two other edges that touch or cross.
Signex draws a custom pad from this outline alone; the pad's size does
not add copper.

## Trapezoid pads

A `trapezoid` pad carries its delta in the optional `trapezoid_delta`
column as `x,y` in whole nanometres, in the pad's own frame. `size` is the
pad's box through its middle. A delta `x` makes the +y edge `size.x + x/2`
long and the -y edge `size.x - x/2`; a delta `y` does the same for the +x
and -x edges along y. So the corners, counter-clockwise from the -x -y
corner with `hw`, `hh` half the size and `qx`, `qy` a quarter of the
delta, are `(-(hw - qx), -(hh - qy))`, `(hw - qx, -(hh + qy))`,
`(hw + qx, hh + qy)`, `(-(hw + qx), hh - qy)`. Signex refuses a trapezoid
with no delta, a delta on both axes, or a delta of at least twice the side
it narrows.

## Zones

A zone is `[[zones]]` with `fill_type` `pour` (copper Signex computes),
`keepout` or `solid` (copper exactly the outline). `outline` is a list of
`{ x, y }` points; `holes` is a list of such lists, each a simple ring
inside the outline and apart from the others.
