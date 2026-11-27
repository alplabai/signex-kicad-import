# Symbol library fixtures

Written by hand for these tests; they are not copies of any KiCad library.
The format follows the public documentation listed in
`docs/KICAD_SYMBOL_FORMAT.md`. Licence: same as the repository (GPL-3.0-or-later).

- `basic.kicad_sym`: plain symbol with the four mandatory properties, description, keywords, footprint filters.
- `multiunit.kicad_sym`: a dual gate with common unit 0, De Morgan body styles 1 and 2, unit names, hidden pins, pin name settings, every graphic kind.
- `alternates.kicad_sym`: pins with `alternate` functions, an old-form bare `hide`, and a new-form `(hide yes)`.
- `extends.kicad_sym`: parents, derived symbols (one with a missing parent, one cycle), a repeated name.
- `power.kicad_sym`: a power symbol, flags `in_bom` / `on_board` / `exclude_from_sim`.
- `unknown.kicad_sym`: tokens the reader does not know and elements it cannot read.
- `Parts.kicad_symdir/`: the per-symbol form; one symbol per file, a derived symbol in a file apart from its parent, a file that is not a symbol, and a truncated file.
- `parts.kicad_sym`: the symbols the part rows are built from: a resistor with every property kind, a derived symbol, a part with no footprint but filters, one whose footprint does not exist, a power symbol, a LED, an unknown prefix.
