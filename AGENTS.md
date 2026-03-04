# AGENTS.md
Last updated: 2026-03-04. Covers: Tickets 0-70.1.

## Project
Proto GE: a prototype-first Rust game engine + colony-sim vertical slice (RimWorld-inspired).  
Primary goals: ship a playable vertical slice fast, keep seams clean, and optimize for low-end PCs through architecture (not premature complexity).

## Roles
- Codex: implements tickets and makes code changes.
- ChatGPT (5.2 Thinking): plans architecture, writes specs/tickets, reviews diffs/logs.
- Human: chooses priorities, runs builds/tests, resolves product decisions.

## North Star (Vertical Slice)
1) Launch engine (window + loop)  
2) Load a scene  
3) Spawn a few entities with transforms  
4) Player input moves something  
5) Camera renders it  
6) Debug overlay (FPS, TPS, counts)  
7) Clean quit  

Everything else is optional until this works.

## Non-negotiables
- Prototype-friendly: simplest thing that works, then iterate.
- Clean seams: engine layer vs game rules layer must stay separate.
- Determinism mindset: simulation should be stable under fixed timestep; do not introduce concurrency into simulation early.
- Performance mindset: avoid per-tick allocations and full scans; time-slice systems later; cache queries.
- No large refactors unless a ticket explicitly requires it.

## Content Pipeline Decision (Locked In)
- Authoring format: XML (mods + base content).
- Runtime format: compiled binary ContentPack cached on disk.
- Behavior: compile-on-first-launch. If inputs change, recompile. If cache is missing/corrupt/version-mismatched, rebuild.
- Rule: runtime simulation must never parse or traverse XML.

## Repo Conventions
- Use small, incremental commits/changes aligned to a single ticket.
- Keep code modular by module responsibility (App/Loop, Scene, Rendering, Assets, Input, Tools).
- Prefer clear, explicit data structures over clever abstractions.
- This file is intentionally kept at repo root as the start-here workflow hub.
- All longform project docs live under `docs/`.

## Doc Date Hygiene (Mandatory)
- Any time Codex edits a doc that has a `Last updated:` header, update that date to the current local date (America/Los_Angeles).
- If that doc also includes `Covers: Tickets ...` or similar coverage metadata, update it when relevant.

## Doc Map / Source of Truth
Read in this order for docs navigation:
1) `AGENTS.md` (root start-here workflow hub)
2) `README.md` (human entry point and run basics)
3) `docs/PROTOGE_INFRASTRUCTURE_REFERENCE.md` (runtime baseline contract)
4) `docs/content_pipeline_contract_v1.md` (content/XML contract)
5) `docs/CODEXNOTES.md` (living context)
6) `docs/CODEXNOTES_ARCHIVE.md` (ticket-by-ticket historical logs)

### Primary sources (contracts)
- Runtime/architecture: `docs/PROTOGE_INFRASTRUCTURE_REFERENCE.md`
- Content contract: `docs/content_pipeline_contract_v1.md`
- CLI details: `docs/thruport_cli.md`
- Console command schemas: `docs/CONSOLE_COMMANDS.md`
- Test targeting / harness guidance: `docs/test_helper.md`

### Workflow helpers (non-authoritative)
- Thruport session workflow helpers: `.codex_artifacts/SOME_COMMANDS.md`

If guidance conflicts, the Primary sources (contracts) override Workflow helpers.

## Repo-first workflow (Required)
Before implementing any ticket, Codex must inspect repo reality and use it as the primary source of truth.

Inspect first, in this order:
1) Rules and overview
- AGENTS.md
- docs/CODEXNOTES.md
- `docs/V2_BACKLOG.md` (review before ticket work; append entries when ROADMAP has V2 bullets)
- `.codex_artifacts/SOME_COMMANDS.md` (canonical thruport start-session workflow)
- README.md and docs/ (if present)

2) Build, dependency, and run commands
- Build system entry points (Cargo.toml, build scripts, CI workflows, and similar)

3) Relevant source areas
- src/, engine/, runtime/, tools/, tests/ (or closest equivalents)

If anything is unclear, Codex must report what it found and propose a minimal plan before editing code.

## CODEXNOTES.md (Living Context)
Codex may read and update `docs/CODEXNOTES.md` to preserve structured context across threads.  
Rules for editing:
- Append-only by default; if you revise, keep prior decisions and mark them as deprecated instead of deleting.
- Use concise bullet points and headings.
- Record decisions, interfaces, file paths, and pitfalls discovered.
- Do not store large code dumps; store summaries and links to files/functions.
- Ticket-by-ticket logs must be written directly to `docs/CODEXNOTES_ARCHIVE.md` at ticket completion time.
- `docs/CODEXNOTES.md` must not be used as a temporary staging area for ticket logs.
- Codex should update `docs/CODEXNOTES.md` after every ticket only if net-new living context changed (decision, file path convention, interface/data contract, performance rule, or pitfall).

Suggested sections inside docs/CODEXNOTES.md:
- Decisions (locked)
- Current milestone and next tickets
- Module map (what owns what)
- Data contracts (Scene/Entity, DefDatabase, ContentPack v1, cache keys)
- Performance rules of thumb
- Known issues / TODO

## Ticket Format
When implementing work, follow the ticket’s “CODEX INPUT” section only.  
If there is a “NOTES” section, treat it as guidance and constraints, not extra scope.

### Ticket structure (Required)
Every ticket must use exactly these top-level sections, in this order:
- `CODEX INPUT`
- `ROADMAP`
- `NOTES`

### ROADMAP V2 capture rule (Required)
- `ROADMAP` must include an explicit `V2` subsection (`V2: None` is allowed).
- If `ROADMAP` includes one or more V2 bullets, append each bullet as its own entry in `docs/V2_BACKLOG.md` in the matching module section.
- If `ROADMAP` says `V2: None`, do not modify `docs/V2_BACKLOG.md`.
- Use the strict template in `docs/V2_BACKLOG.md` for every appended entry.

### V2 backlog archival rule (Required)
- `docs/V2_BACKLOG.md` is active backlog only (not-yet-implemented work).
- Completed or obsolete V2 entries must be moved to `docs/V2_BACKLOG_ARCHIVE.md` (move, do not copy).
- Each archived entry must include `Date (YYYY-MM-DD)` and `Closed-by` metadata.

### Required per-ticket sections (Non-negotiable)
Every CODEX INPUT must include these subsections.

DO NOT (mandatory)
- Collateral edits forbidden.
- No broad refactors.
- No formatting-only changes.
- No renames unless required by the ticket.
- Do not touch files outside the allowed list (or outside the minimal set of files required for the ticket).
- Do not introduce new dependencies unless the ticket explicitly requires it and you update manifests accordingly.

VERIFICATION (mandatory)
- Provide concrete commands to run (build, test, lint or fmt if applicable).
- Include expected outcomes.
- If commands are unknown, discover the correct commands from build files and CI configs before implementation.
- If verification fails, fix and re-run until passing.

## How to Work (Codex Operating Rules)
- Before editing: restate the ticket goal and list the files you expect to touch.
- Implement the smallest version that satisfies acceptance criteria.
- Prefer adding a minimal test or smoke check when feasible.
- When blocked: write a short note in `docs/CODEXNOTES.md` describing what you found, what you tried, and the next step.

### Implementation behavior rules (Anti-slop)
- No guessing: do not silently assume architecture, file locations, commands, or dependencies. If repo inspection does not resolve a question, pick the safest minimal path and state the assumption clearly in the ticket or `docs/CODEXNOTES.md`.
- No “catfish code”: do not produce code that looks plausible but is not wired into real project structure, real build steps, or real runtime behavior.
- Small, verifiable steps: implement the smallest working slice first, verify, then extend. Do not attempt large end-to-end builds in one step.

### Architecture invariants checklist (Required for any non-trivial change)
Answer yes/no and include mitigation if any risk is “yes”:
- Does this introduce new global state or singletons?
- Does this change dependency direction (engine depending on game, engine depending on tools)?
- Does this require cross-module knowledge that breaks boundaries?
- Is the seam demonstrably extensible or swappable without editing unrelated modules?
- Can it be verified without manual guesswork?

## Hard Cuts (Do Not Add Yet)
- Full ECS frameworks
- Multithreaded simulation or job system
- Complex mod patch language / inheritance systems
- Networking
- Advanced rendering features (lighting, animation pipelines)
- Editors and tool UIs beyond the debug overlay

## Definition of Done
A ticket is done when:
- Acceptance criteria are met
- Build/run succeeds
- Any added tests pass
- VERIFICATION commands in the ticket were run and passed
- `docs/CODEXNOTES.md` is updated only for net-new living context, and ticket logs are written to `docs/CODEXNOTES_ARCHIVE.md`

## docs/CODEXNOTES.md vs docs/CODEXNOTES_ARCHIVE.md (Separation and Usage)

Purpose and scope
- `docs/CODEXNOTES.md` is the living context file. It must stay short, current, and immediately useful for implementing the next tickets.
- `docs/CODEXNOTES_ARCHIVE.md` is historical record. It stores ticket logs and superseded decisions so the main file does not become noisy.

What belongs in docs/CODEXNOTES.md
Keep only information that is actively needed to make correct changes right now:
- Locked decisions that are still in force
- Current milestone and next-ticket queue
- Current module map (who owns what)
- Active data contracts and interfaces (only the latest versions)
- Active performance rules of thumb
- Current known issues / pitfalls that still apply
- References to where things live (file paths, key functions), kept concise

What belongs in docs/CODEXNOTES_ARCHIVE.md
Move anything that is not needed for near-term work:
- Ticket-by-ticket logs written at ticket completion time
- Deprecated or replaced decisions (keep them for history, but marked deprecated)
- Old interface versions and old schema versions that are no longer supported
- One-off investigations and debugging timelines that are solved

When to use each file
- Read `docs/CODEXNOTES.md` first before any implementation work. Treat it as current truth.
- Consult `docs/CODEXNOTES_ARCHIVE.md` only when you need history:
  - Why a decision was made
  - How an interface evolved
  - Tracking regressions to an earlier change
  - Recovering a previously removed approach

Update rules (mandatory)
- After each ticket, write the ticket log directly to `docs/CODEXNOTES_ARCHIVE.md`.
- After each ticket, update `docs/CODEXNOTES.md` only with net-new, still-relevant living context: decisions, contracts, file paths, pitfalls.
- If you revise an existing living note, prefer marking the old one as deprecated and moving deprecated detail into `docs/CODEXNOTES_ARCHIVE.md`, keeping `docs/CODEXNOTES.md` clean.

## Developer Notes: `docs/CONSOLE_COMMANDS.md`

`docs/CONSOLE_COMMANDS.md` is a developer-facing reference for the in-game console. It is meant for you (and anyone working on the project) to quickly remember what commands exist, what they do, and how to use them during testing and iteration. It is not a public-facing spec and does not need to describe internal implementation details.

When to use `docs/CONSOLE_COMMANDS.md`
Use it as the first stop when you want to:

* Spawn or despawn entities while testing
* Reset or switch scenes while iterating
* Inspect or nudge gameplay state during debugging
* Remember exact syntax, optional arguments, and defaults
* Confirm what a command prints on success or failure

What to put in `docs/CONSOLE_COMMANDS.md`
Keep entries practical and copy-paste friendly:

* Command name
* One-line description
* Syntax line
* A few example invocations
* Notes about defaults (for example: where something spawns if no position is provided)
* Notes about safety or limitations (for example: “debug builds only”, “may break determinism tests”)

Project rules for edits

* If you add, remove, or change a console command, update `docs/CONSOLE_COMMANDS.md` in the same change.
* If the console shell/input behavior changes (toggle key, history behavior, submission/queue semantics), update `docs/CONSOLE_COMMANDS.md` in the same change.
* Prefer documenting the behavior you observe in-game (inputs and outputs) rather than how it’s implemented.
* If behavior is intentionally unstable while prototyping, mark it clearly as “temporary” so it’s not mistaken for a guarantee.

Ownership and routing reminder
Commands may be handled at different layers:

* Engine/scene-machine commands (quit, reset scene, switch scene)
* Scene/game commands (spawn, despawn, teleport, etc.)

`docs/CONSOLE_COMMANDS.md` should note which layer a command targets, mainly so you know what scene needs to be active for it to work.


