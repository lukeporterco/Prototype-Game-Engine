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

## How to Work (Codex Operating Rules)
- Before editing: restate the ticket goal and list the files you expect to touch.
- Implement the smallest version that satisfies acceptance criteria.
- Prefer adding a minimal test or smoke check when feasible.
- When blocked: write a short note in CODEXNOTES.md describing what you found, what you tried, and the next step.

## Hard Cuts (Do Not Add Yet)
- Full ECS frameworks
- Multithreaded simulation or job system
- Complex mod patch language / inheritance systems
- Save/load system
- Networking
- Advanced rendering features (lighting, animation pipelines)
- Editors and tool UIs beyond the debug overlay

## Definition of Done
A ticket is done when:
- Acceptance criteria are met
- Build/run succeeds
- Any added tests pass
- CODEXNOTES.md is updated if a decision, interface, or pitfall was discovered

I don't want you to touch GPTContext.md EVER unless I give explicit instructions to update it.