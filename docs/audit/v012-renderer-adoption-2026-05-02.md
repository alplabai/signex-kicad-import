# v0.12 cleanroom-renderer adoption — orientation

**Date:** 2026-05-02
**Importer repo:** `signex-kicad-import` (GPL-3.0)
**Consumer repo:** `../signex` @ `feature/v0.12-cleanroom-rewrite` (Apache-2.0)
**Sources read (this repo):** `crates/cli/src/main.rs`,
`crates/kicad-parser/src/schematic.rs`,
`crates/kicad-writer/src/schematic.rs`,
`crates/signex-types/src/{schematic.rs, format.rs}`, top-level `Cargo.toml`,
`README.md`.
**Sources read (consumer, read-only):**
`docs/audit/cleanroom-rewrite-2026-05-01.md`, `docs/RENDERING_RULES.md`,
`crates/signex-types/src/schematic.rs`, `crates/signex-types/src/format.rs`
(only `SymbolExtras` block),
`crates/signex-render/src/schematic/{mod, pin, field_style}.rs`,
`crates/signex-engine/src/transform.rs` (autoplace lives here, not in
signex-render — the user prompt's path was stale).

---

## What the importer currently emits

The importer is a thin pipe: `kicad-parser` builds a
`signex_types::schematic::SchematicSheet` directly from the KiCad
`(symbol …)` s-expression, then `SnxSchematic::new(sheet).write_string()`
serialises it to the TOML-envelope + TSV-bulk `.snxsch` format. There is
**no dependency on `signex-render`** — the importer only consumes
`signex-types`, which is **vendored at this repo's
`crates/signex-types/`**. KiCad's `(fields_autoplaced)` token is mapped
1:1 to `Symbol::fields_autoplaced` in
`crates/kicad-parser/src/schematic.rs:999-1002` and persisted through
`SymbolExtras::fields_autoplaced` in `format.rs:1207`. Pin geometry is
copied verbatim (no padding / trimming — `PIN_LENGTH_MM` is only used as a
default when KiCad omits the length token at line 728), `lib_id` is always
emitted as-found (no "skip if unresolved" path), and no render-style enum
strings (`PowerPortStyle::Standard`, etc.) are emitted because those are
runtime options, not document data. The PCB importer is also a pure pipe.

## Where the importer diverges from v0.12 expectations

The single material gap is **the vendored `signex-types` is locked to the
pre-v0.12 schema** and is missing `Symbol::fields_user_placed: bool` (the
one-line v0.12 type addition listed in the consumer audit doc) at both
`crates/signex-types/src/schematic.rs:390-434` and
`crates/signex-types/src/format.rs:1189-1228` (`SymbolExtras`). Because
the field is `#[serde(default)]` in v0.12, the .snxsch files this importer
emits today still **load cleanly** in the v0.12 signex-app — but they
load with `fields_user_placed = false`. That means the v0.12 autoplacer
in `signex-engine::transform::autoplace_fields` (called from every
rotate / mirror per `transform.rs:323, 383`) is **allowed to overwrite
the field positions imported from KiCad** the moment the user rotates or
mirrors a symbol. The autoplace tie-break is `Bottom > Top > Left > Right`,
which is deliberately disjoint from KiCad's, so the post-rotate layout
will visibly differ from what the user authored in KiCad. Items C, D, E,
F, G in the prompt all check out without importer changes (pin AABB is
computed at render time from intrinsic `Pin` data — no imported padding
to retract; `lib_id` is already preserved verbatim; PCB stays a pipe;
no render enums emitted; the importer touches no `signex-render`
deprecated shims because it doesn't depend on `signex-render` at all).

## Minimal patch to close the gap

Three small, mechanical changes, in dependency order:

1. **Vendored types — add `fields_user_placed`.** In
   `crates/signex-types/src/schematic.rs::Symbol`, insert
   `#[serde(default)] pub fields_user_placed: bool` next to
   `fields_autoplaced` (mirrors consumer line 422). In
   `crates/signex-types/src/format.rs::SymbolExtras`, insert the same
   field plus update `is_default()` (`&& !self.fields_user_placed`) and
   `from_symbol()` / `row_to_symbol()` (mirrors consumer lines 1205,
   1239, 1262, 1382). All `#[serde(default)]` so legacy `.snxsch` files
   keep loading.
2. **Parser — set the new field.** In
   `crates/kicad-parser/src/schematic.rs` near line 1100, set
   `fields_user_placed: !fields_autoplaced` when constructing
   `Symbol{…}`. Rationale: KiCad's `(fields_autoplaced)` token means
   "KiCad's autoplacer placed these — they are not user-curated", so
   the inverse is the closest semantic match for Signex's
   user-placed flag. This preserves manually-positioned KiCad fields
   across the user's first Signex rotate/mirror while still letting
   Signex re-autoplace fields that KiCad itself had auto-placed.
   (The user prompt floated `fields_user_placed = true` always; that
   is more conservative but loses information KiCad encoded.
   Recommend `!fields_autoplaced` — willing to switch to `true`-always
   if you'd prefer maximum preservation.)
3. **Writer — keep compilation green.** Adding a `Symbol` field forces
   `crates/kicad-writer/src/schematic.rs` test fixtures (lines 1261,
   1474) and any local `Symbol{…}` literal in tests to add
   `fields_user_placed: …`. The writer's actual sexpr emit pass at
   `schematic.rs:288-290` does **not** need a new branch — KiCad has
   no `fields_user_placed` token, so the round-trip back out to
   `.kicad_sch` is unchanged.

Out of scope but flagged: the vendored `Symbol` is also missing the v0.9
library-pinning fields (`library_id`, `row_id`, `library_version`) the
consumer's `schematic.rs:454-468` carries. Not required for v0.12
rendering; worth a follow-up vendor refresh once `signex-types` ships
to crates.io (per the `Cargo.toml` `TODO(release)` comment).

---

## Verification (2026-05-02)

Patches applied in three commits (vendored types, parser mapping, and
this audit doc). Workspace baseline:

- `cargo build --workspace` ✓
- `cargo test --workspace` ✓ (29 + 16 + 35 + 2 new = 82 lib tests pass;
  doc-tests 0/0)
- Two new focused parser tests assert the mapping in both directions:
  `symbol_without_fields_autoplaced_marks_fields_user_placed` and
  `symbol_with_fields_autoplaced_clears_fields_user_placed`.

`cargo fmt --check` and `cargo clippy --workspace -- -D warnings`
report 11 collapsible-if errors in `crates/signex-types/src/markup.rs`
and a long-message wrap in `crates/signex-types/src/project.rs`. These
are **pre-existing on `master` HEAD `bd325b7`** — verified by `git
stash` + re-run before restoring my changes — and untouched by this
patch. Out of scope to fix here; flagging for a follow-up `chore(deps):
vendor signex-types lints` cleanup.

**End-to-end render round-trip not performed** — no `.kicad_sch`
fixture lives in either repo, and I'm not introducing one without your
approval (binary fixtures bloat git history and may carry licensing
implications the GPL-3.0 / Apache split was set up to avoid). Suggested
next step: drop a known-good KiCad fixture into `tests/fixtures/` and
I'll convert it + open the result in `cargo run --release -p signex-app`
from `../signex` for a visual diff.

