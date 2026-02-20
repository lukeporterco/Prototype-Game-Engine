# Content Pipeline Contract v1

Status: Locked for Ticket 6 (spec-first).  
Scope: Contract only. No compiler or loader implementation in this ticket.

## 1. Goal and Boundary

This contract defines the seam between XML authoring and runtime content use:

1. Authoring format is XML.
2. Compiler output is `ContentPack v1` (one pack per mod).
3. Loader output is runtime `DefDatabase`.
4. Runtime simulation consumes `DefDatabase` only and never parses XML.

The purpose is to keep runtime fast and constrained, while preserving deterministic and debuggable content compilation.

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

### 3.1 `mod_id` source (explicit)

1. For entries under `mods/`, `mod_id` is the leaf folder name.
2. No aliasing/remapping is allowed in v1.
3. Base content uses fixed `mod_id = "base"` from `assets/base`.

Examples:

1. `mods\betterlabels` -> `mod_id = "betterlabels"`
2. `mods\foo_bar` -> `mod_id = "foo_bar"`
3. `assets\base` -> `mod_id = "base"`

### 3.2 XML discovery root

1. Compiler discovers XML files recursively under a mod root.
2. Input paths are normalized to forward slashes (`/`) for hashing and deterministic ordering.

## 4. Def Philosophy and Minimal Def Type

Defs are archetypes only, with no runtime mutable state.

The first locked def type is `EntityDef`.

`EntityDef` v1 fields:

1. `defName: string` required.
2. `label: string` required.
3. `renderable` required.
   - Preferred form:
     - `<renderable kind="Placeholder" />`
     - `<renderable kind="Sprite" spriteKey="player" />`
   - Legacy-compatible text form (still accepted):
     - `<renderable>Placeholder</renderable>`
     - `<renderable>Sprite:<key></renderable>`
   - `Sprite` keys use `[a-z0-9_/-]`, must be non-empty, and must not include `..`, leading `/`, or `\`.
4. `moveSpeed: f32` optional, default `5.0`, must be finite and `>= 0`.
5. `tags: list<string>` optional.

Minimal shape:

```xml
<Defs>
  <EntityDef>
    <defName>proto.player</defName>
    <label>Player</label>
    <renderable kind="Placeholder" />
    <moveSpeed>5.0</moveSpeed>
    <tags>
      <li>colonist</li>
      <li>starter</li>
    </tags>
  </EntityDef>
</Defs>
```

## 5. Validation Rules

Validation is strict. Content errors fail load/compile for that run.

Rules:

1. Missing required fields are errors.
2. Unknown elements/fields are errors.
3. Invalid enum values are errors.
4. Non-finite or invalid numeric values are errors.
5. Duplicate key in the same mod is an error (`(def_type, def_name)` duplicate).
6. Cross-mod key collision with different `def_type` is an error.

## 6. Error Model

Compiler and loader report structured `ContentError` entries:

1. `code`
2. `message`
3. `mod_id`
4. `file_path`
5. best-effort `line` and `column`
6. optional `def_type`
7. optional `def_name`
8. optional `hint`

If parser location is unavailable, use `source_location = unknown` while keeping `mod_id` and `file_path`.

## 7. Deterministic Ordering Rules

### 7.1 Compiler ordering

1. Discover XML files recursively.
2. Sort by normalized relative path (lexicographic).
3. Within each file, read defs in document order.
4. Before serialization, sort compiled defs by `(def_type, def_name)`.

### 7.2 Loader ordering

1. Base pack applies first.
2. Mods apply in configured load order.
3. Runtime numeric IDs are assigned after final merge, sorted by `(def_type, def_name)`.

## 8. ContentPack v1

Per-mod binary format with required header fields:

1. `magic: [u8; 4] = "PGCP"`
2. `pack_format_version: u16 = 1`
3. `compiler_version: String`
4. `game_version: String`
5. `mod_id: String`
6. `mod_load_index: u32`
7. `enabled_mods_hash_sha256: [u8; 32]`
8. `input_hash_sha256: [u8; 32]`
9. `def_count: u32`
10. `source_file_count: u32`
11. `created_utc_unix_seconds: u64` (diagnostic only)
12. `ordering_scheme: "v1:path-lex+doc-order+def-sort"`

Body requirements:

1. Serialized defs.
2. Source mapping records sufficient for mod/file level diagnostics.

## 9. Cache Invalidation Contract

Pack validity depends on exact match of these inputs:

1. `pack_format_version`
2. `compiler_version`
3. `game_version`
4. `mod_id`
5. `mod_load_index`
6. `enabled_mods_hash_sha256`
7. `input_hash_sha256` (from normalized relative path + raw file bytes)

Additional rules:

1. Missing cache -> rebuild.
2. Corrupt cache -> rebuild.
3. Version mismatch -> rebuild.
4. `compiler_version` invalidation uses exact string equality.
5. `game_version` invalidation uses exact string equality.
6. Any byte difference in either string invalidates cache.
7. No semver/range compatibility in v1.

## 10. Override Rules

Override key is `(def_type, def_name)`.

1. Later mod wins over earlier mod.
2. Scalar fields use last-writer-wins.
3. List fields replace the whole field (no append/deep merge).
4. Duplicate key in same mod is error.
5. Same `def_name` with different `def_type` across mods is error.

## 11. Runtime `DefDatabase` Contract

1. Runtime stores compiled defs only.
2. Runtime uses numeric IDs in hot paths.
3. Runtime does not parse XML.
4. Runtime may maintain precomputed indices.

## 12. Fixture Set

See `docs/fixtures/content_pipeline_v1/` and `docs/fixtures/content_pipeline_v1/EXPECTATIONS.md` for pass/fail cases, merged expectations, and invalidation checks.

## 13. Non-goals for v1

1. No deep patch language.
2. No semver compatibility windows for cache reuse.
3. No runtime XML fallback.
4. No full gameplay schema beyond minimum `EntityDef` slice contract.
