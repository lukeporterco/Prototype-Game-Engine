# AGENTS.md

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

## Repo-first workflow (Required)
Before implementing any ticket, Codex must inspect repo reality and use it as the primary source of truth.

Inspect first, in this order:
1) Rules and overview
- AGENTS.md
- CODEXNOTES.md
- `.codex_artifacts/SOME_COMMANDS.md` (canonical thruport start-session workflow)
- README.md and docs/ (if present)

2) Build, dependency, and run commands
- Build system entry points (Cargo.toml, build scripts, CI workflows, and similar)

3) Relevant source areas
- src/, engine/, runtime/, tools/, tests/ (or closest equivalents)

If anything is unclear, Codex must report what it found and propose a minimal plan before editing code.

## CODEXNOTES.md (Living Context)
Codex may read and update `CODEXNOTES.md` to preserve structured context across threads.  
Rules for editing:
- Append-only by default; if you revise, keep prior decisions and mark them as deprecated instead of deleting.
- Use concise bullet points and headings.
- Record decisions, interfaces, file paths, and pitfalls discovered.
- Do not store large code dumps; store summaries and links to files/functions.
- Codex should update CODEXNOTES.md after every ticket if any of the following changed: a decision, a file path convention, an interface/data contract, a performance rule, or a new pitfall/bug pattern was discovered.

Suggested sections inside CODEXNOTES.md:
- Decisions (locked)
- Current milestone and next tickets
- Module map (what owns what)
- Data contracts (Scene/Entity, DefDatabase, ContentPack v1, cache keys)
- Performance rules of thumb
- Known issues / TODO

## Ticket Format
When implementing work, follow the ticket’s “CODEX INPUT” section only.  
If there is a “NOTES” section, treat it as guidance and constraints, not extra scope.

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
- When blocked: write a short note in CODEXNOTES.md describing what you found, what you tried, and the next step.

### Implementation behavior rules (Anti-slop)
- No guessing: do not silently assume architecture, file locations, commands, or dependencies. If repo inspection does not resolve a question, pick the safest minimal path and state the assumption clearly in the ticket or CODEXNOTES.md.
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
- CODEXNOTES.md is updated if a decision, interface, file path convention, performance rule, or pitfall was discovered

## CODEXNOTES.md vs CODEXNOTES_ARCHIVE.md (Separation and Usage)

Purpose and scope
- `CODEXNOTES.md` is the living context file. It must stay short, current, and immediately useful for implementing the next tickets.
- `CODEXNOTES_ARCHIVE.md` is historical record. It stores older ticket logs and superseded decisions so the main file does not become noisy.

What belongs in CODEXNOTES.md
Keep only information that is actively needed to make correct changes right now:
- Locked decisions that are still in force
- Current milestone and next-ticket queue
- Current module map (who owns what)
- Active data contracts and interfaces (only the latest versions)
- Active performance rules of thumb
- Current known issues / pitfalls that still apply
- References to where things live (file paths, key functions), kept concise

What belongs in CODEXNOTES_ARCHIVE.md
Move anything that is not needed for near-term work:
- Completed ticket-by-ticket logs after they’re no longer relevant
- Deprecated or replaced decisions (keep them for history, but marked deprecated)
- Old interface versions and old schema versions that are no longer supported
- One-off investigations and debugging timelines that are solved

When to use each file
- Read `CODEXNOTES.md` first before any implementation work. Treat it as current truth.
- Consult `CODEXNOTES_ARCHIVE.md` only when you need history:
  - Why a decision was made
  - How an interface evolved
  - Tracking regressions to an earlier change
  - Recovering a previously removed approach

Update rules (mandatory)
- After each ticket, update `CODEXNOTES.md` only with net-new, still-relevant context: decisions, contracts, file paths, pitfalls.
- If an update would add more than a short set of bullets or it is primarily historical detail, put it in `CODEXNOTES_ARCHIVE.md` and add a brief pointer from `CODEXNOTES.md` (one or two bullets) explaining what moved and why.
- If you revise an existing note, prefer marking the old one as deprecated and moving the deprecated detail into `CODEXNOTES_ARCHIVE.md`, keeping `CODEXNOTES.md` clean.

## Developer Notes: `CONSOLE_COMMANDS.md`

`CONSOLE_COMMANDS.md` is a developer-facing reference for the in-game console. It is meant for you (and anyone working on the project) to quickly remember what commands exist, what they do, and how to use them during testing and iteration. It is not a public-facing spec and does not need to describe internal implementation details.

When to use `CONSOLE_COMMANDS.md`
Use it as the first stop when you want to:

* Spawn or despawn entities while testing
* Reset or switch scenes while iterating
* Inspect or nudge gameplay state during debugging
* Remember exact syntax, optional arguments, and defaults
* Confirm what a command prints on success or failure

What to put in `CONSOLE_COMMANDS.md`
Keep entries practical and copy-paste friendly:

* Command name
* One-line description
* Syntax line
* A few example invocations
* Notes about defaults (for example: where something spawns if no position is provided)
* Notes about safety or limitations (for example: “debug builds only”, “may break determinism tests”)

Project rules for edits

* If you add, remove, or change a console command, update `CONSOLE_COMMANDS.md` in the same change.
* If the console shell/input behavior changes (toggle key, history behavior, submission/queue semantics), update `CONSOLE_COMMANDS.md` in the same change.
* Prefer documenting the behavior you observe in-game (inputs and outputs) rather than how it’s implemented.
* If behavior is intentionally unstable while prototyping, mark it clearly as “temporary” so it’s not mistaken for a guarantee.

Ownership and routing reminder
Commands may be handled at different layers:

* Engine/scene-machine commands (quit, reset scene, switch scene)
* Scene/game commands (spawn, despawn, teleport, etc.)

`CONSOLE_COMMANDS.md` should note which layer a command targets, mainly so you know what scene needs to be active for it to work.
