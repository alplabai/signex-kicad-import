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
| unit `NAME_U_S` | `part_number` U (0 = every unit); style 1 -> 1, 2 -> 2, 0 -> 0 (both); other styles dropped (`symbol.body_style_dropped`); a unit above 255 dropped (`symbol.unit_dropped`: Signex stores units in one byte) |
| `part_count` | highest unit written, at least 1, never above 255 |
| pin electrical type | `Input Output Bidirectional Tristate Passive Unspecified Power PowerOutput OpenCollector OpenEmitter NotConnected`; `free` -> `Unspecified` (approximated) |
| pin graphic style | Signex reads one decoration column for the symbols a schematic draws, the outside edge, so the style goes there: `inverted` -> `Dot`; `clock` -> `ClockEdge`; `input_low` / `output_low` -> `ActiveLowInput` / `ActiveLowOutput`. A style that combines a marker with a clock wedge keeps the marker (`inverted_clock` -> `Dot`, `clock_low` -> `ActiveLowInput`) and is approximated; `edge_clock_high` -> `ClockEdge` and `non_logic` -> none are approximated. The other three columns are written `None` |
| pin angle 0/90/180/270 | `Right`/`Up`/`Left`/`Down`: the direction the pin extends from its connection point toward the body, which is KiCad's meaning too |
| pin name `~` | empty name, name not visible |
| alternates `(alternate "N" type style)` | the pin's `function` cell: names joined with `\|` (the format's separator; `\|` in a name becomes `/`). Their types and styles are not kept (approximated) |
| `pin_numbers hide` / `pin_names hide` / per-text `hide` | `designator_visible` / `name_visible` false |
| rectangle, circle, text | `rectangle`, `circle`, `text` (text size = font height; rotation not carried) |
| arc (start, mid, end; a fill is dropped, an arc is not closed; collinear points: a line, `symbol.arc_degenerate`) | `arc`: centre, radius, counter-clockwise `start_deg`..`end_deg` in `[0, 360)`; drawn clockwise -> start and end swap |
| polyline | closed (first = last) -> `polygon`; open -> `line` segments (a fill on an open one is dropped) |
| bezier | `bezier` (token `snxsym/v4`); Signex never fills one, so a fill is dropped (`symbol.fill_dropped`) |
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
| pad types smd / thru_hole / np_thru_hole / connect | `Smd` / `Tht` / `NptHole` / `ConnectorPad`; `pad_prop_fiducial_glob` / `_loc` -> `Fiducial` (layers by side: copper and mask only); `pad_prop_castellated` -> `Castellated` (the through-hole set); `pad_prop_bga`, `_testpoint`, `_heatsink` have no field (approximated) |
| pad layers | the named sets of 4.2 in order: `Top Layer`, `Bottom Layer`, `Top Solder`, `Bottom Solder`, `Top Paste`, `Bottom Paste` (a pad that lacks paste lacks the token); a non-plated hole keeps mask layers only (`pad.npth_copper_dropped`) |
| shapes | `round`, `rect`, `oval`, `round_rect:<ratio>`, `trapezoid:<dx>,<dy>`, `custom:...` (chamfered corners stay the board converter's rounded corners) |
| `solder_mask_margin`, `solder_paste_margin` (pad, else the footprint's) | the pad's base margin columns; a paste ratio (pad or footprint) has no field (`pad.paste_ratio_not_carried`) |
| `(drill oval DIAMETER WIDTH)` | `drill_diameter` = the smaller, `drill_slot_length` = the larger; the format does not say which way a slot lies, it is taken along the pad's x axis, so a slot longer in y is approximated (`pad.slot_orientation_approximated`) |
| footprint on `B.Cu` | refused (error): turning a flipped footprint back needs the mirrored pad and text turns, which the format's frame rules do not pin down for a standalone file |
| fp_line / fp_rect / fp_circle / fp_arc / fp_poly / fp_text on silk, fab | `silk_f`, `silk_b`, `fab_f`, `fab_b` (line, rectangle, circle, arc, polygon, text); an arc runs from +x toward +y as the format says; text rotation not carried |
| courtyard drawings | `courtyard`: a polygon or rectangle as is, four-plus lines chained into the ring, else the bounding box (approximated) |
| other layers | dropped (`footprint.graphic_layer_dropped`) |
| `descr`, `tags` | `description`; tags at its end (approximated) |
| `attr exclude_from_bom` | `component_type` `StandardNoBom`; other flags: approximated |
| Reference prefix (`REF**` is no prefix) | `default_designator` |
| `model` | `model.deferred` (info); 3D models come with the model unit |
| zones | dropped (`footprint.zone_dropped`) |

A footprint also gets the format's default `body_3d` (Extrude, 1.0 mm, the default colours), written explicitly so the file is the same whether or not a reader treats the key as optional.

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
| `symbol.fill_approximated`, `symbol.arc_degenerate`, `symbol.text_rotation_dropped`, `symbol.text_sanitized` | approximated | see the table |
| `symbol.fill_dropped`, `symbol.unit_dropped` | dropped | see the table |
| `symlib.duplicate_name` | error | a second symbol library with the same name; not read |
| `footprint.unreadable` | error | the file does not parse |
| `footprint.duplicate_name` | error | a footprint name repeated; the first is kept |
| `footprint.file_ignored` | info | |
| `footprint.invalid_utf8` | approximated | bytes replaced |
| `footprint.zone_dropped`, `footprint.graphic_layer_dropped`, `footprint.graphic_unreadable`, `pad.layer_dropped` | dropped | |
| `pad.paste_ratio_not_carried`, `pad.property_not_carried`, `pad.slot_orientation_approximated`, `footprint.courtyard_approximated`, `footprint.tags_in_description`, `footprint.attr_not_carried`, `footprint.text_rotation_dropped`, `pad.npth_copper_dropped`, `pad.text_sanitized` | approximated | |
| `model.deferred` | info | 3D model not converted yet |
| `library.table_not_read` | error | `sym-lib-table` / `fp-lib-table` inputs are not read yet |

## Open points

1. Which way a slot drill lies in a Signex pad is not in the format.
2. `--update` replaces changed files and deletes the symbol and footprint files (and an old manifest) of items that left the source, restoring everything if a step fails. It does not yet bump part versions or keep part `created` times (part rows are regenerated from the source).
3. Whether a library with no part tables loads in Signex is not stated.
4. Envelope times are the epoch (see above); `updated` is not set on `--update`.
5. Library tables are not read; `--models` / `--models-dir` are accepted and have no effect until models are converted.
