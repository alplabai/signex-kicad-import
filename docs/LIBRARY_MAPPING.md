# KiCad library to Signex library: the mapping

What `signex-kicad-import library` writes, and where it approximates. The
Signex side comes only from the public library format
(`docs/LIBRARY_FORMAT.md` in the Signex repository; section numbers below
refer to it); the KiCad side from `KICAD_SYMBOL_FORMAT.md` and the public
KiCad file-format documentation. No Signex or KiCad source was read.

## Library directory

```text
<output>/
  <Name>.snxlib                 manifest, classes and one part table per source symbol library
  symbols/<slug>.snxsym         one envelope per source symbol library
  footprints/<uuid>.snxfpt      one footprint per file, named by its uuid
  import-report.json / .md
```

3D models are later work. Written files are staged beside `--output` and moved in only when the run succeeds.

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
| Reference, Value positions and text | `designator_pos`, `comment_pos`; designator = Reference + `?`; the comment is the source Value text (placement shows it when a part has no MPN), `*` when there is none. A derived row shares its parent's symbol, so shows the parent's Value: `part.value_from_parent` (approximated) when the values differ |
| `in_bom no` | `component_type` `StandardNoBom` |
| `power`, `on_board no`, `exclude_from_sim yes` | no field: approximated items |
| `extends` (a derived symbol) | no symbol of its own: a part row on the parent's symbol (below) |
| Datasheet, Footprint, keywords, filters, other properties | part-row data (below); not in the symbol file |

The token is the lowest the file needs (3.5). Text in pin cells may hold no
control character or `'''` (open point 3 of the format): they become spaces
(`symbol.text_sanitized`).

## Part rows (`.snxlib`)

One table per source symbol library, named after the library (a character a
table name cannot hold, `/ \ . : * ? " < > |`, becomes `_`; a clash gets
`~2`; `part.table_name_changed`). Rows follow section 2 of the format: the 22
columns, rows sorted by `row_id`, compact JSON cells, `content_hash` of
section 6 (`snx_writer::part_table`, checked byte for byte against the
conformance tables and hash documents). A library with no rows has no table.

| Row field | From |
|---|---|
| `row_id` | UUIDv5 of the library id, `part`, library name, symbol name |
| `internal_pn` | `<library>:<symbol>` |
| `class` | the leading letters of Reference: R resistor, C capacitor, L inductor, D diode, LED led, Q transistor, U / IC ic, J / P / CN connector, SW / S switch, Y / X crystal, H / MH mechanical, FID fiducial, else generic. Only the classes used are declared in the manifest's `[[classes]]` (key and label, in the order of this list) |
| `symbol_ref` | the symbol (for a derived symbol, the nearest ancestor that draws) |
| `footprint_ref` | the `Footprint` property `Lib:Name`: the footprint of that name converted from the library called `Lib` (a table's nickname, or a `.pretty`'s name). A named footprint that was not converted skips the row (`part.footprint_missing`, error). An empty property: no footprint |
| `datasheet` | `Datasheet` as a url; `~` or empty is none |
| `description` | `Description` (or `ki_description`) |
| `primary_mpn` | `Manufacturer` / `MPN` (or `MFR`, `Manufacturer_Name`, `Manufacturer_Part_Number`, `MFG_PN`); empty text when the symbol has none |
| `parameters` (text) | `designator_prefix`, `value` (Value), `keywords` (`ki_keywords`), `footprint_filters` (`ki_fp_filters`, only when there is no footprint), every other non-empty property under its own name (`<name>_property` for a name that clashes with one of these) |
| state, version, released | `Released`, `1.0.0`, `true`; symbol and footprint versions `1.0.0` |
| `created`, `updated` | the epoch (no clock) |

A power symbol (its own `power` flag or its ancestor's) gets a symbol and no
row. Rows have no alternates, supply, simulation model or pin map.

## Library tables

`sym-lib-table` and `fp-lib-table` (inputs by their file names) list libraries by
nickname, type, uri, options and description. Each `KiCad` entry is converted
under its nickname, which is what `Lib:Name` footprint properties name. The uri
may use `${VAR}` variables, but only from an allowlist: `${KIPRJMOD}` (the
table's folder), KiCad's own `KICAD<n>_SYMBOL_DIR` / `_FOOTPRINT_DIR` /
`_3DMODEL_DIR` (from the environment) and any `NAME` given as
`--search-path NAME=DIR`. No other variable is ever read, so a table cannot
make the converter expose the environment; errors show the uri as written.
(Table cells, pin and pad cells never hold control characters or `'''`: they
become spaces, reported as `part.text_sanitized` / `symbol.text_sanitized`.) an unset variable or a missing path falls back to the rest
of the path, then the file name, below each `--search-path` folder. Entries
that cannot be used are error items and never stop the others: an unresolved uri
(`library.table_entry_unresolved`), a type other than `KiCad`
(`library.table_type_unsupported`), an unreadable table
(`library.table_unreadable`). A `(disabled)` entry is skipped
(`library.table_entry_disabled`, info). Each resolved library is also listed in
the report's `sources`.

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
| `(drill oval DIAMETER WIDTH)` | `drill_diameter` = the smaller, `drill_slot_length` = the larger; the slot lies along the pad's x axis; one longer in y on a round, rect, oval or roundrect pad is made exact by swapping the pad size and adding 90 degrees to its rotation, on a trapezoid or custom pad it is approximated (`pad.slot_orientation_approximated`). Signex cannot place slotted drills on boards yet, so every footprint with a slot gets `pad.slot_not_placeable` (approximated); the slot is still written exactly, not rounded to a hole (decision: Caner (2026-11-26); board placement support is signex #3128) |
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
| `pad.paste_ratio_not_carried`, `pad.property_not_carried`, `pad.slot_orientation_approximated`, `pad.slot_not_placeable`, `footprint.courtyard_approximated`, `footprint.tags_in_description`, `footprint.attr_not_carried`, `footprint.text_rotation_dropped`, `pad.npth_copper_dropped`, `pad.text_sanitized` | approximated | |
| `model.deferred` | info | 3D model not converted yet |
| `part.written` | info | the row was written; `id` is its `row_id` |
| `part.footprint_missing` | error | the named footprint was not converted; row skipped |
| `part.table_name_changed`, `part.text_sanitized` | approximated | |
| `library.table_entry_unresolved`, `library.table_type_unsupported`, `library.table_unreadable` | error | see Library tables |
| `library.table_entry_disabled` | info | |
| `library.table_entry_substituted` | info | a variable was unset and the file was found below a `--search-path` folder; names the uri and the file read |
| `library.table_entry_unreadable` | error | the resolved library could not be read (I/O); other entries go on |
| `part.value_from_parent` | approximated | see Symbols |
| `update.library_kept` | info | see open point 2 |

## Open points

1. Which way a slot drill lies in a Signex pad is not in the format.
2. `--update` follows contract section 5. It reads what the earlier run wrote (its report's `files` and `file_libraries`, the old `.snxlib` rows by `row_id`, the old envelopes). It deletes only files the earlier report names, and only if every entry of that report is trusted: written by this converter, and exactly `symbols/<name>.snxsym`, `footprints/<uuid>.snxfpt` or a root `<name>.snxlib`, regular files (never links) whose content matches their name. One bad entry means nothing is deleted in the run (`update.report_entry_ignored`, info). A file is deleted only when its source library read without an error this run, or is no longer an input (`update.library_removed`, info: its files and part rows go; the snapshot rule). A library that is an input but failed to read keeps its files and part rows (`update.library_kept`, info). User files are never touched. A table's `--search-path` fallback never follows `..` out of its folder; error messages show the uri as written, not an expanded path. A symbol or footprint whose content changed gets a patch version bump and the run's time as `updated`, `created` kept; unchanged ones keep their bytes. A part row with an unchanged `content_hash` (and unchanged symbol and footprint versions) keeps its bytes; a changed one gets a patch bump and `updated` = the run's time, and keeps `created`, state and `released`; the rows' `symbol_version` / `footprint_version` follow the primitives. Everything is moved aside first and put back if a step fails; a backup left by a crashed run is restored before the next one starts. An update is a snapshot: the app guards uncommitted work.

**Ledger.** The report records what the converter wrote itself (`ledger`: row ids with their library, symbol ids, footprint ids; also `files` / `file_libraries`), validated like the file list (uuids, plain names without `:`). An update deletes only recorded items: a row, table, class, file or footprint the user added is carried over byte for byte (a table with columns of its own verbatim; a symbol envelope holding symbols the user added is not rewritten, `update.envelope_has_user_items`). Not preserved: the manifest's own settings (users, workflow, description) and sidecars of user tables (column types, prices) are written afresh; junctions on Windows are untested. Staging restores a crashed run's backup only if the leftover folder belongs to the current user (checked by creating a file and reading its owner; off unix nothing is restored and the user is told to inspect it), and re-checks every path component for links just before each move (not atomic; `*at` calls through cap-std would close the race).
3. Whether a library with no part tables loads in Signex is not stated.
4. Envelope times are the epoch (see above); `updated` is not set on `--update`.
5. `--models` / `--models-dir` are accepted and have no effect until models are converted.
6. Part rows carry no manufacturer data unless the symbol has `Manufacturer` / `MPN`; the primary part's `mpn` and `manufacturer` are then empty text, which the format allows only as far as it says nothing against it.
