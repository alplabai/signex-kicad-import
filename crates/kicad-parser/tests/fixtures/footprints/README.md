# Footprint library fixtures

Written by hand for these tests; not copies of any KiCad library. They follow
the public file-format documentation (dev-docs.kicad.org, footprint library
section) and the footprint files observed in public projects. Licence: same
as the repository (GPL-3.0-or-later).

- `Test.pretty/R_Test_0603.kicad_mod`: SMD chip with rounded pads, mask margin, silk, fab, a courtyard drawn as four lines, description, tags, a 3D model.
- `Test.pretty/Conn_Test_2.kicad_mod`: through-hole connector: rect and rotated oval pads, a non-plated hole, circle, arc, polygon courtyard, text, a zone, `exclude_from_bom`.
- `Test.pretty/Broken.kicad_mod`: truncated file.
- `Test.pretty/notes.txt`: not a footprint.
