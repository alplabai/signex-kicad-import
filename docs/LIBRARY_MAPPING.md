# KiCad library to Signex library: the mapping

What `signex-kicad-import library` writes, and where it approximates. The
Signex side comes only from the public library format
(`docs/LIBRARY_FORMAT.md` in the Signex repository; section numbers below
refer to it); the KiCad side from `KICAD_SYMBOL_FORMAT.md` and the public
KiCad file-format documentation. No Signex or KiCad source was read.

## Library directory

```text
<output>/
  <Name>.snxlib                 manifest only (section 2.1), no part tables yet
  symbols/<slug>.snxsym         one envelope per source symbol library
  footprints/<uuid>.snxfpt      one footprint per file, named by its uuid
  import-report.json / .md
```

Part rows (the `[tables.*]` of the `.snxlib`) and 3D models are later work; a
library written now has symbols and footprints but no parts. Written files are
staged beside `--output` and moved in only when the run succeeds.

Ids are UUIDv5, never random (`kicad_parser::libids`): the library id from the
display name and the input **file names** (not paths); a symbol, footprint or
envelope id from the library id, the source library name and the item name.
`--library-id` replaces the library id. Envelope `created` / `updated` are the
Unix epoch: a conversion has no clock, and the same input must give the same
bytes.

## Symbols (`.snxsym`)

Frames: both KiCad symbol libraries and Signex symbols are +y up
(section 3), so coordinates and angles carry over unchanged. (Only the
footprint frame differs, below.)

| KiCad | Signex |
|---|---|
| symbol with `extends` | its own properties, the drawing of the nearest ancestor |
| unit `NAME_U_S` | `part_number` U (0 = every unit); style 1 -> 1, 2 -> 2, 0 -> 0 (both); other styles dropped (`symbol.body_style_dropped`) |
| `part_count` | highest unit, at least 1 |
| pin electrical type | `Input Output Bidirectional Tristate Passive Unspecified Power PowerOutput OpenCollector OpenEmitter NotConnected`; `free` -> `Unspecified` (approximated) |
| pin graphic style | decoration columns: `inverted` -> outside edge `Dot`; `clock` -> inside edge `ClockEdge`; `inverted_clock` both; `input_low` / `output_low` -> outside edge `ActiveLowInput` / `ActiveLowOutput`; `clock_low` -> clock edge + `ActiveLowInput`; `edge_clock_high` -> clock edge; `non_logic` -> none. The format names the tokens but not their columns; this assignment follows the usual conventions (open point 1) |
| pin angle 0/90/180/270 | `Right`/`Up`/`Left`/`Down`: the direction the pin extends from its connection point toward the body, which is KiCad's meaning too |
| pin name `~` | empty name, name not visible |
| alternates `(alternate "N" type style)` | the pin's `function` cell: names joined with `\|` (the format's separator; `\|` in a name becomes `/`). Their types and styles are not kept (approximated) |
| `pin_numbers hide` / `pin_names hide` / per-text `hide` | `designator_visible` / `name_visible` false |
| rectangle, circle, text | `rectangle`, `circle`, `text` (text size = font height; rotation not carried) |
| arc (start, mid, end) | `arc`: centre, radius, counter-clockwise `start_deg`..`end_deg` in `[0, 360)`; drawn clockwise -> start and end swap |
| polyline | closed (first = last) -> `polygon`; open -> `line` segments |
| bezier | `bezier` (token `snxsym/v4`) |
| fill `background` / `outline` | Signex fill colour (255,255,194,255) / black (approximated); `(fill (type color))` exact |
| Reference, Value positions | `designator_pos`, `comment_pos`; designator = Reference + `?` |
| `in_bom no` | `component_type` `StandardNoBom` |
| `power`, `on_board no`, `exclude_from_sim yes` | no field: approximated items |
| Datasheet, Footprint, keywords, filters, other properties | part-row data (later); not in the symbol file |

The token is the lowest the file needs (3.5). Text in pin cells may hold no
control character or `'''` (open point 3 of the format): they become spaces
(`symbol.text_sanitized`).

## Footprints (`.snxfpt`)

Each `.kicad_mod` is converted by the board converter's footprint code in a
library context (no nets, a default layer table, placed at the origin on the
top side), so pads, shapes and angles follow one rule set. The frame needs no
flip: KiCad footprints and Signex footprints are both +y down (section 4), and
KiCad's counter-clockwise-on-screen angles become Signex's from-+x-toward-+y
angles by negation, which the converter's frame handling does (a pad at 90
degrees is 270 in the file).

| KiCad | Signex |
|---|---|
| pad types smd / thru_hole / np_thru_hole / connect | `Smd` / `Tht` / `NptHole` / `ConnectorPad` |
| pad layers | the named sets of 4.2 in order: `Top Layer`, `Bottom Layer`, `Top Solder`, `Bottom Solder`, `Top Paste`, `Bottom Paste` (a pad that lacks paste lacks the token); a non-plated hole keeps mask layers only (`pad.npth_copper_dropped`) |
| shapes | `round`, `rect`, `oval`, `round_rect:<ratio>`, `trapezoid:<dx>,<dy>`, `custom:...` (chamfered corners stay the board converter's rounded corners) |
| `solder_mask_margin`, `solder_paste_margin` | the pad's base margin columns |
| fp_line / fp_rect / fp_circle / fp_arc / fp_poly / fp_text on silk, fab | `silk_f`, `silk_b`, `fab_f`, `fab_b` (line, rectangle, circle, arc, polygon, text); an arc runs from +x toward +y as the format says; text rotation not carried |
| courtyard drawings | `courtyard`: a polygon or rectangle as is, four-plus lines chained into the ring, else the bounding box (approximated) |
| other layers | dropped (`footprint.graphic_layer_dropped`) |
| `descr`, `tags` | `description`; tags at its end (approximated) |
| `attr exclude_from_bom` | `component_type` `StandardNoBom`; other flags: approximated |
| Reference prefix (`R**` is no prefix) | `default_designator` |
| `model` | `model.deferred` (info); 3D models come with the model unit |
| zones | dropped (`footprint.zone_dropped`) |

A footprint also gets the format's default `body_3d` (extrude, 0.6 mm), since
the files in the conformance set always carry one (open point 3).

## Report codes

Symbol reader codes are in `KICAD_SYMBOL_FORMAT.md`; board message codes in
`REPORT_CODES.md`. The mapping adds:

| Code | Severity | Meaning |
|---|---|---|
| `symbol.written`, `footprint.written` | info | the item was written; `id` is its uuid |
| `symbol.power_not_carried` | approximated | power flag has no field |
| `symbol.on_board_not_carried`, `symbol.exclude_from_sim_not_carried` | approximated | no field |
| `symbol.body_style_dropped` | dropped | style above 2 |
| `symbol.pin_type_approximated`, `symbol.pin_style_approximated`, `symbol.pin_angle_approximated`, `symbol.pin_alternates_approximated` | approximated | see the table |
| `symbol.fill_approximated`, `symbol.text_rotation_dropped`, `symbol.text_sanitized` | approximated | see the table |
| `footprint.unreadable` | error | the file does not parse |
| `footprint.duplicate_name` | error | a footprint name repeated; the first is kept |
| `footprint.file_ignored`, `footprint.invalid_utf8` | info | |
| `footprint.zone_dropped`, `footprint.graphic_layer_dropped`, `pad.layer_dropped`, `footprint.polygon_dropped` | dropped | |
| `footprint.courtyard_approximated`, `footprint.tags_in_description`, `footprint.attr_not_carried`, `footprint.text_rotation_dropped`, `pad.npth_copper_dropped`, `pad.text_sanitized` | approximated | |
| `model.deferred` | info | 3D model not converted yet |
| `library.table_not_read` | error | `sym-lib-table` / `fp-lib-table` inputs are not read yet |

## Open points

1. Which decoration column takes `ActiveLow*` / `ClockEdge` is not in the format; confirm with the Signex maintainers.
2. `--update` overwrites files in place and does not delete items that left the source, and does not bump part versions (there are no part rows yet).
3. Whether a footprint without `body_3d`, or a library with no part tables, loads in Signex is not stated; the writer errs on the side of writing what the fixtures have.
4. Envelope times are the epoch (see above); `updated` is not set on `--update`.
5. Library tables are not read; `--models` / `--models-dir` are accepted and have no effect until models are converted.
