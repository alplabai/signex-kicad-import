# Report codes

Stable machine codes of the structured report (contract section 4). A code
never changes meaning. `{x}` marks the variable part of a message.

## Board conversion messages

`kicad_parser::message_codes` maps each free-text board warning to one of
these (`detail` carries `what` for the variable part, and `count`).

| Code | Severity | Message |
|---|---|---|
| `zone.outline_missing` | dropped | zone without an outline dropped |
| `zone.pad_connection_approximated` | approximated | zone that leaves pads unconnected written with direct pad connections |
| `zone.layer_dropped` | dropped | zone on a non-copper layer dropped from that layer |
| `zone.corner_unreadable` | dropped | zone corner that does not read dropped |
| `zone.area_split` | approximated | zone area apart from its first outline written as its own zone |
| `zone.hatched_approximated` | approximated | hatched zone written as a solid pour |
| `zone.rule_area_approximated` | approximated | rule area that allows some items written as a full copper keepout |
| `footprint.zone_moved` | approximated | footprint zone written as a board zone |
| `via.position_missing` | dropped | via without a readable position dropped |
| `via.size_missing` | dropped | via without a positive drill and size dropped |
| `track.ends_missing` | dropped | track without its ends dropped |
| `copper_drawing.written_as_tracks` | approximated | unfilled copper drawing written as tracks |
| `copper_drawing.points_missing` | dropped | copper drawing without its points dropped |
| `copper_drawing.width_missing` | dropped | copper drawing without a positive width dropped |
| `copper_arc.points_missing` | dropped | copper arc without its three points dropped |
| `pad.trapezoid_both_axes` | approximated | trapezoid pad narrowing on both axes written as a custom outline |
| `pad.trapezoid_delta_wide` | approximated | trapezoid pad delta wider than the pad written as a custom outline |
| `pad.slot_approximated` | approximated | slot drill kept as a round hole of its width, marked oval |
| `pad.outline_not_simple` | approximated | pad outline not a simple polygon; anchor shape kept |
| `pad.custom_hole_filled` | approximated | hole in custom pad copper filled |
| `pad.drill_offset_ignored` | dropped | drill offset from the pad centre ignored |
| `pad.custom_outline_width_ignored` | dropped | custom pad polygon outline width ignored |
| `pad.custom_corner_unreadable` | dropped | custom pad polygon corner that does not read dropped |
| `pad.custom_copper_unmerged` | approximated | custom pad copper that does not merge written as its anchor shape alone |
| `pad.custom_copper_detached` | dropped | custom pad copper apart from the rest of the pad dropped |
| `pad.chamfer_approximated` | approximated | chamfered pad corners written as round corners |
| `pad.position_missing` | dropped | pad without a readable position dropped |
| `pad.size_missing` | dropped | pad without a positive size dropped |
| `stackup.negative_value` | dropped | stackup with a negative thickness or permittivity not written |
| `stackup.layer_repeated` | dropped | stackup layer listed again ignored |
| `stackup.colour_dropped` | dropped | stackup layer colour dropped |
| `stackup.permittivity_missing` | approximated | stackup dielectric without a permittivity written without one |
| `stackup.sublayers_merged` | approximated | stackup dielectric sublayers written as one row |
| `stackup.copper_mismatch` | dropped | stackup copper does not match the board's copper layers; stackup not written |
| `net_class.empty` | dropped | net class with no nets and no patterns dropped |
| `net_class.pattern_incomplete` | dropped | net class pattern without a class or pattern dropped |
| `net_class.pattern_unknown_class` | dropped | net class pattern for a class the project does not define dropped |
| `net_class.name_missing` | dropped | net class without a name dropped |
| `net_class.assignment_unknown_class` | dropped | net class assignment to a class the project does not define dropped |
| `footprint.position_missing` | dropped | footprint without a readable position dropped |
| `footprint.text_position_missing` | dropped | footprint text without a readable position dropped |
| `footprint.polygon_corner_unreadable` | dropped | footprint polygon corner that does not read dropped |
| `footprint.arc_old_form` | dropped | footprint arc in the old angle form dropped |
| `footprint.model_dropped` | dropped | footprint 3D model dropped; the board file has no place for it |
| `footprint.hidden_text_dropped` | dropped | hidden footprint text dropped; a drawing has no hidden state |
| `board.text_position_missing` | dropped | board text without a readable position dropped |
| `board.polygon_corner_unreadable` | dropped | board polygon corner that does not read dropped |
| `board.edge_open` | dropped | board edge does not close; no outline written |
| `board.drawing_points_missing` | dropped | board drawing without its points dropped |
| `field.position_unreadable` | approximated | field position that does not read dropped; the field is kept |
| `field.source_metadata_dropped` | dropped | source-tool metadata field dropped |
| `field.imported_value_dropped` | dropped | imported-library value field dropped |
| `field.imported_value_used` | info | imported-library value field used as the empty Value |
| `id.repeated` | info | repeated uuid replaced with a fresh one |
| `number.non_finite` | approximated | number that is not finite written as 0 |
| `pad.shape_approximated` | approximated | pad shape {x} written as a rectangle |
| `pad.layer_dropped` | dropped | pad layer {x} dropped |
| `pad.custom_primitive_dropped` | dropped | custom pad primitive {x} dropped |
| `footprint.drawing_layer_dropped` | dropped | footprint drawing on layer {x} dropped |
| `board.drawing_layer_dropped` | dropped | board drawing on layer {x} dropped |
| `board.text_layer_dropped` | dropped | board text on layer {x} dropped |
| `footprint.item_dropped` | dropped | footprint {x} dropped |
| `board.item_dropped` | dropped | board {x} dropped |
| `stackup.layer_unmapped` | dropped | stackup copper layer {x} has no Signex layer; dropped |
| `stackup.type_dropped` | dropped | stackup layer type {x} dropped |
| `layer.copper_past_limit` | dropped | copper layer {x} past 32 layers dropped |
| `layer.unmapped` | dropped | layer {x} has no Signex layer; its items are dropped |
| `item.width_missing` | dropped | {x} without a positive width dropped |
| `copper_item.layer_not_copper` | dropped | `{what} on non-copper layer {name} dropped` |

## Symbol library reader

See `KICAD_SYMBOL_FORMAT.md` (codes `symlib.*` and `symbol.*`).
