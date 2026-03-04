# Content Pipeline Contract v1
Last updated: 2026-03-04. Covers: Tickets 6-70.1.

Status: Active and aligned to shipped compiler/runtime behavior.
Scope: Contract for what is currently enforced and relied on in the vertical slice.

## 1. Goal and Boundary

This contract defines the seam between XML authoring and runtime content use:

1. Authoring format is XML.
2. Compiler output is compiled content packs.
3. Loader output is runtime `DefDatabase`.
4. Runtime simulation consumes `DefDatabase` only and never parses XML.

## 2. Public Contract Interfaces

Compiler contract:

```rust
compile_mod_to_content_pack(mod_context, xml_inputs)
    -> Result<ContentPackV1, Vec<ContentError>>
```

Loader contract:

```rust
load_content_packs_to_def_database(base_pack, mod_packs_in_load_order)
    -> Result<DefDatabase, Vec<ContentError>>
```

## 3. Mod Identity and Source Rules

### 3.1 `mod_id` source

1. For entries under `mods/`, `mod_id` is the leaf folder name.
2. No aliasing/remapping is allowed in v1.
3. Base content uses fixed `mod_id = "base"` from `assets/base`.

### 3.2 XML discovery root

1. Compiler discovers XML files recursively under a mod root.
2. Input paths are normalized to forward slashes (`/`) for hashing and deterministic ordering.

## 4. EntityDef v1 (Current Enforced Schema)

Defs are archetypes only, with no runtime mutable state.

Supported `EntityDef` fields:

1. `defName` (required, non-empty text).
2. `label` (required for first/full definition; optional on later overrides).
3. `renderable` (required for first/full definition; optional on later overrides).
4. `moveSpeed` (optional `f32`, finite and `>= 0`, runtime default `5.0`).
5. `health_max` (optional `u32`, must be `> 0` when present).
6. `base_damage` (optional `u32`, `0` allowed).
7. `aggro_radius` (optional `f32`, finite and `>= 0`).
8. `attack_range` (optional `f32`, finite and `>= 0`).
9. `attack_cooldown_seconds` (optional `f32`, finite and `>= 0`).
10. `tags` (optional list of `<li>` text entries only).

### 4.1 `renderable` accepted forms

Attribute form:

- `<renderable kind="Placeholder" />`
- `<renderable kind="Sprite" spriteKey="visual_test/pawn_blue" pixelScale="3" />`

Legacy text form (still accepted):

- `<renderable>Placeholder</renderable>`
- `<renderable>Sprite:visual_test/pawn_blue</renderable>`

Sprite key constraints:

- Must pass key validation (`[a-z0-9_/-]`, non-empty, no leading `/`, no `..`, no `\`).

### 4.2 `renderable` strictness rules

1. Allowed `renderable` attributes: `kind`, `spriteKey`, `pixelScale` only.
2. `kind="Placeholder"` must not include `spriteKey`, `pixelScale`, or child elements.
3. `kind="Sprite"` requires `spriteKey`.
4. `pixelScale` is optional, integer `1..=16`, default `1`.
5. Text-form `renderable` must not include attributes or child elements.

### 4.3 Sprite anchors rules

For `kind="Sprite"`, one optional `<anchors>` block is supported:

```xml
<renderable kind="Sprite" spriteKey="..." pixelScale="3">
  <anchors>
    <anchor name="hand" x="4" y="-1" />
    <anchor name="carry" x="3" y="-2" />
    <anchor name="tool" x="4" y="-1" />
  </anchors>
</renderable>
```

Rules:

1. At most one `<anchors>` block.
2. `<anchors>` has no attributes.
3. `<anchors>` children must be `<anchor>` only.
4. `<anchor>` allowed attributes: `name`, `x`, `y` only (all required).
5. `name` allowed values: `hand`, `carry`, `muzzle`, `light_origin`, `tool`.
6. `x` and `y` must parse as `i16` integers.
7. Duplicate anchor names are rejected.

## 5. Validation Strictness and Unknown-Field Behavior

Validation is strict. Unknown fields/elements/attributes are rejected with compile errors, including nested unknowns:

1. Unknown fields in `<EntityDef>` are rejected.
2. Unknown attributes/children in `<renderable>` are rejected.
3. Unknown children in `<tags>` are rejected (only `<li>` allowed).
4. Unknown attributes/children in `<anchors>` and `<anchor>` are rejected.

Additional validation rules:

1. Missing required fields are errors.
2. Duplicate fields in one `EntityDef` are errors.
3. Invalid enum values are errors.
4. Non-finite/invalid numeric values are errors.
5. Duplicate `defName` in the same mod is an error.

## 6. Override and Merge Rules

Override key is `(def_type, def_name)`.

1. Later mod wins over earlier mod.
2. Scalar fields use last-writer-wins.
3. List fields replace the whole field (no append/deep merge).
4. Partial override is allowed only if a prior definition exists.
5. First/full definition must include `label` and `renderable`.

## 7. Deterministic Ordering Rules

### 7.1 Compiler ordering

1. Discover XML files recursively.
2. Sort by normalized relative path (lexicographic).
3. Within each file, read defs in document order.

### 7.2 Loader ordering

1. Base applies first.
2. Mods apply in configured load order.
3. Runtime IDs are assigned from merged defs in stable sorted order.

## 8. Runtime `DefDatabase` Contract

1. Runtime stores compiled defs only.
2. Runtime hot paths use numeric IDs.
3. Runtime does not parse XML.

## 9. Fixture Set

See `docs/fixtures/content_pipeline_v1/` and `docs/fixtures/content_pipeline_v1/EXPECTATIONS.md` for pass/fail cases and regression coverage.

## 10. Non-goals for v1

1. No deep patch language.
2. No runtime XML fallback.
3. No schema version bump in this contract update.
