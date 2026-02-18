# CODEXNOTES.md

## Purpose
Living, structured notes for Codex and humans to keep consistent context across threads.
Keep this concise and actionable. Prefer bullet points. Avoid long code dumps.

---

## Decisions (Locked)
- Language/runtime: Rust
- Content authoring: XML
- Content runtime: compiled binary ContentPack
- Compile behavior: compile-on-first-launch, cache per mod, rebuild on change or invalid cache
- Simulation: fixed timestep (TPS), deterministic mindset
- Vertical slice priorities: boot → scene → entity move → render → debug overlay → clean quit

---

## Current Milestone
- Milestone: Vertical Slice v0
- Definition: window + loop + scene + controllable entity + render + overlay + clean quit

### Next Tickets (Queue)
- Ticket 0: Repo skeleton and one-command run
- Ticket 1: App loop lifecycle
- Ticket 2: Scene boundary and minimal entity model
- Ticket 3: Input mapping and controllable entity
- Ticket 4: Minimal rendering and camera seam
- Ticket 5: Tools overlay
- Ticket 6: Content pipeline contract
- Ticket 7: Mod discovery + compile plan
- Ticket 8: XML MVP compiler → DefDatabase
- Ticket 9: ContentPack v1 cache load/save
- Ticket 10: Override rule MVP

---

## Module Map (Ownership)
- Core: common types, IDs, error/reporting primitives
- App/Loop: window init, main loop, fixed timestep, lifecycle
- Scene: scene lifecycle, entity storage, spawn/despawn, world state container
- Rendering: camera, renderables, world-to-screen
- Input: action mapping, per-frame input sampling
- Assets: (placeholder) asset handles and paths, later async loading
- Tools: debug overlay, counters, diagnostics
- Content: (later) mod scan, XML compile, pack IO, DefDatabase

---

## Data Contracts (Plain English)

### Scene and Entity (runtime)
- Entity: stable ID + Transform + Renderable descriptor (placeholder)
- Transform: position (2D for now), optional rotation later
- Scene API: load / update(fixed_dt) / render / unload
- Scene owns entity list and spawn/despawn rules

### Content and Mods
- Mod: folder with XML files (and optionally art assets later)
- Load order: base content first, then mods in configured order
- Override rule MVP: last one wins for scalar fields

### DefDatabase (runtime)
- Stores compiled “Defs” (archetypes only, no runtime state)
- Uses numeric IDs internally (no string lookups in hot paths)
- Precomputed indices allowed (by tag/category/etc) when needed

### ContentPack v1 (cached binary)
- Per-mod cache file
- Includes: pack version, compiler version, input hash, deterministic ordering rules
- Includes mapping back to source file info for errors/debugging (minimal)

### Cache Key (invalidation)
- Inputs to cache validity:
  - Game version
  - Compiler version
  - Mod load order position / enabled mod list hash
  - Hash of all XML files in the mod (paths + contents)

---

## Performance Rules of Thumb
- Avoid per-tick allocations in simulation loop
- Avoid scanning all entities each tick; prefer caches/indices and time-slicing
- Separate sim update cadence from render cadence
- Do not introduce multithreading into simulation until profiling proves need
- Treat content parsing as load-time only; runtime never touches XML

---

## Logging and Error Model
- Errors should be actionable: mod name, file path, line/field if possible, and a clear message
- Content errors fail fast at load (don’t limp into runtime with partial state)
- Keep logs structured and minimal in hot paths

---

## Pitfalls and Gotchas (Update as found)
- (empty)

---

## Known Issues / TODO
- (empty)

---

## Change Log (Optional)
- YYYY-MM-DD: short note of major decisions or interface changes

---

## Ticket Notes (2026-02-18)
- Ticket 0 implemented: workspace bootstrap + deterministic startup path resolution.
- New workspace layout:
  - `Cargo.toml` workspace root with members `crates/engine` and `crates/game`
  - `crates/engine`: startup/bootstrap contracts and path resolution
  - `crates/game`: binary entrypoint, banner, logging, clean exit behavior
  - Canonical directories created: `assets/base/`, `mods/`, `docs/`
- Startup path contract added in code (`crates/engine/src/lib.rs`):
  - `AppPaths { root, base_content_dir, mods_dir, cache_dir }`
  - `resolve_app_paths() -> Result<AppPaths, StartupError>`
- Root resolution rules (locked for now):
  - If `PROTOGE_ROOT` is set: validate as repo marker root
  - Else walk upward from executable directory and choose first marker match
  - Marker definition: directory containing `Cargo.toml` and either `crates/` or `assets/`
  - Never resolve from current working directory
- Failure UX contract:
  - If root cannot be found, fail fast with explicit instructions to set `PROTOGE_ROOT` and include PowerShell + Bash examples.
- Runtime directory behavior:
  - `cache/` is runtime-managed and created via `create_dir_all` at startup.

## Pitfalls and Gotchas (Update as found)
- Local environment pitfall: `cargo` command may be unavailable in shell PATH even when repo scaffold is valid. Validation commands could not be executed in this session.

## Change Log (Optional)
- 2026-02-18: Ticket 0 scaffolded workspace crates and deterministic root discovery from executable ancestors with `PROTOGE_ROOT` override.
