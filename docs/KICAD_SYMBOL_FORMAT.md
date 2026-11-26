# KiCad symbol library: what the reader relies on

`kicad_parser::symlib` reads `.kicad_sym` and `.kicad_symdir` into an
in-memory model. It was written without reading KiCad's source code. Each
format fact below names where it comes from: the public file-format
documentation (D), the KiCad manual (M), or observation of library files
themselves (O), which is how a file's author-visible tokens can be seen without
any program source.

D = <https://dev-docs.kicad.org/en/file-formats/sexpr-symbol-lib/> and
<https://dev-docs.kicad.org/en/file-formats/sexpr-intro/> (sections Symbols,
Symbol Properties, Symbol Graphic Items, Symbol Pin, Symbol Unit Identifier,
Fill Definition). Last modified 2025-02-20 on the page read.

| Fact | Source |
|---|---|
| A library is `(kicad_symbol_lib (version YYYYMMDD) (generator G) SYMBOL...)`, zero or more symbols | D |
| `(symbol "ID" ...)`: a top-level symbol, or a unit child named `NAME_UNIT_STYLE`; unit 0 is common to all units; body styles are 1 and 2 only | D |
| `(extends "PARENT")` names another symbol of the same library; a derived symbol differs only in its properties | D |
| `(pin_numbers hide)`, `(pin_names [(offset N)] hide)`; default offset 0.508 mm | D |
| `(in_bom yes\|no)`, `(on_board yes\|no)`; the reader defaults both to yes when absent | D (token), O (default) |
| Properties `(property "KEY" "VALUE" (id N) (at X Y A) (effects ...))`; mandatory Reference, Value, Footprint, Datasheet; reserved keys `ki_keywords`, `ki_description`, `ki_locked`, `ki_fp_filters` | D |
| Graphic items: arc (start mid end), circle (center radius), bezier (4 points), polyline (pts), rectangle (start end), text; each with stroke and fill (`none`, `outline`, `background`) | D |
| Pin: `(pin TYPE STYLE (at X Y A) (length L) (name "N" EFFECTS) (number "N" EFFECTS))`; angles 0, 90, 180, 270; the 12 electrical types and 9 graphic styles listed in `model.rs` | D |
| `(unit_name "...")` inside a unit child | D |
| `(power)` marks a power symbol; newer files write `(power global)` or `(power local)` | O |
| `(exclude_from_sim yes\|no)` | O |
| `(alternate "NAME" TYPE STYLE)` inside a pin: another function of the pin | O |
| A hidden pin or property: a bare `hide` in older files, `(hide yes)` in newer ones; a text can also be hidden in `(effects ... hide)` | O |
| `(fill (type color) (color R G B A))`; `(stroke ... (color R G B A))` | O |
| Property `Description` (newer files) beside the reserved `ki_description` | O |
| `.kicad_symdir`: a folder of per-symbol files; the reader assumes each is a `*.kicad_sym` holding one symbol, either inside `(kicad_symbol_lib ...)` or as a bare `(symbol ...)`. `extends` may name a symbol in another file of the folder. The folder layout is **not** in the pages read above; this is an assumption to confirm against real v10 folders | assumption |

Coordinates are millimetres with the file's axes (y up in a symbol file); the
reader converts nothing.

## What is read

Symbols with `extends`, units (`NAME_U_S`; unit 0 common; style 1 and 2,
other styles kept and reported as `info`), pins (electrical type, style,
position, angle, length, name, number, hidden flags, alternates), the six graphic
kinds with stroke and fill, properties (all of them, with position and effects;
accessors for Description, `ki_keywords`, `ki_fp_filters`), `power` and its
scope, `in_bom`, `on_board`, `exclude_from_sim`, `pin_numbers` / `pin_names`
settings. `effective_units` follows `extends` to the drawing a derived symbol
uses. Graphics written straight in a symbol (outside any unit, which KiCad does
not write) are kept as unit 0, style 0.

## Report codes of the reader

| Code | Severity | Meaning |
|---|---|---|
| `symlib.unreadable` | error | an input file does not parse (CLI; library item) |
| `symlib.file_unreadable` | error | a file of a `.kicad_symdir` does not parse |
| `symlib.file_ignored` | info | a file of a `.kicad_symdir` that is not `*.kicad_sym` |
| `symlib.token_unknown` | info | an unknown token directly in the library |
| `symbol.token_unknown` | info | an unknown token directly in a symbol |
| `symbol.unit_token_unknown` | info | an unknown token (for example `text_box`) in a unit |
| `symbol.name_missing` | error | a symbol without a name |
| `symbol.duplicate_name` | error | a name repeated in the library; the first is kept |
| `symbol.extends_missing` | error | the parent is not in the library |
| `symbol.extends_cycle` | error | the `extends` chain loops |
| `symbol.unit_unreadable` | dropped | a unit name that is not `NAME_UNIT_STYLE` |
| `symbol.unit_name_mismatch` | info | a unit not named after its symbol (kept) |
| `symbol.body_style_beyond_two` | info | a body style above 2 (kept) |
| `symbol.items_outside_unit` | info | graphics or pins outside any unit (kept) |
| `symbol.property_unreadable` | dropped | a property without a name |
| `symbol.pin_unreadable` | dropped | a pin without a position |
| `symbol.pin_type_unknown` | info | an unlisted electrical type, read as `unspecified` |
| `symbol.pin_style_unknown` | info | an unlisted graphic style, read as `line` |
| `symbol.pin_angle_unusual` | info | a pin angle other than 0, 90, 180, 270 |
| `symbol.pin_length_missing` | info | a pin without a length, read as 0 |
| `symbol.graphic_unreadable` | dropped | a graphic item without its geometry |
| `symbol.fill_unknown` | info | an unlisted fill type, read as `none` |

`info` here means the reader lost nothing it understood; a later writer unit may
raise the severity when it cannot carry something on.
