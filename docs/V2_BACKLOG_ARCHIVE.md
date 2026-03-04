# V2 Backlog Archive (Canonical)

This file stores expired, completed, or obsolete entries moved out of `docs/V2_BACKLOG.md`.

Archive rules:
- `docs/V2_BACKLOG.md` is active backlog only (not-yet-implemented work).
- Move entries here when they are completed, obsolete, or otherwise no longer actionable.
- Move, do not copy: remove the entry from `docs/V2_BACKLOG.md` when archiving it.
- Keep the original active-backlog entry text verbatim under the archive metadata block.

Required metadata header for each archived entry:
- Date: `YYYY-MM-DD`
- Closed-by: `Ticket #`, `PR #`, or commit hash (use existing repo convention)
- Category: must match a section in `docs/V2_BACKLOG.md`
- Note: one sentence describing why this entry was archived

Archive entry format example:
```md
### [Archived entry title]
- Date: 2026-03-03
- Closed-by: Ticket 69
- Category: Tools
- Note: Implemented and verified in the ticket verification pass.

### [Original entry title from docs/V2_BACKLOG.md]
- Date: YYYY-MM-DD
- Source: <ticket id or doc link>
- Area: <...>
- Summary: <...>
- Rationale: <...>
- Dependencies: <...>
- Risks: <...>
- Cut: <...>
```

## Core
No archived entries yet.

## App/Loop
No archived entries yet.

## Scene
### [Action state interact/carry emission]
- Date: 2026-03-04
- Closed-by: Ticket 67
- Category: Scene
- Note: Implemented via the Ticket 67 Scene V2 rollup and verified in game tests.

### Action state interact/carry emission
- Date: 2026-02-28
- Source: Ticket 62.1
- Area: Scene
- Summary: Emit `Interact` and `Carry` action visuals for one interactable workflow.
- Rationale: Extend the MVP Idle/Walk visual contract so renderer can reflect core interaction intent.
- Dependencies: Stable interaction-state to action-state mapping for player workflow.
- Risks: Visual-state drift if action emission is not aligned with deterministic gameplay tick boundaries.
- Cut: No blend trees, IK, ragdolls, or physics-driven animation.

### [Visual sandbox second interactable lane]
- Date: 2026-03-04
- Closed-by: Ticket 67
- Category: Scene
- Note: Implemented in the visual sandbox as part of Ticket 67 deterministic demo expansion.

### Visual sandbox second interactable lane
- Date: 2026-02-28
- Source: Ticket 62.4
- Area: Scene
- Summary: Add a second deterministic interactable lane in `visual_sandbox` with role-stable payload semantics.
- Rationale: Expand demo coverage for interaction-state visuals without changing command schema or scenario id.
- Dependencies: Stable visual-sandbox spawn ordering and interaction target resolution by save-id.
- Risks: Scenario clutter can reduce readability if overlap/layout is not kept intentional.
- Cut: No new console commands and no scenario payload schema changes.

### [Visual sandbox deterministic hit-kick trigger]
- Date: 2026-03-04
- Closed-by: Ticket 67
- Category: Scene
- Note: Implemented with deterministic hit trigger behavior during visual sandbox interactions.

### Visual sandbox deterministic hit-kick trigger
- Date: 2026-02-28
- Source: Ticket 62.4
- Area: Scene
- Summary: Add a deterministic demo trigger path that forces `Hit` action visual state in `visual_sandbox`.
- Rationale: Ensure hit-kick polish can be validated visually in a repeatable scene.
- Dependencies: Existing renderer-side `Hit` procedural support and deterministic sandbox state rules.
- Risks: Trigger timing may conflict with ongoing interaction states if priority rules are unclear.
- Cut: No combat-system redesign and no non-deterministic/random trigger logic.

### [Multi-select settlers move order]
- Date: 2026-03-04
- Closed-by: Ticket 67
- Category: Scene
- Note: Implemented deterministic box-select and stable fan-out move ordering in Ticket 67.

### Multi-select settlers move order
- Date: 2026-02-28
- Source: Ticket 63
- Area: Scene
- Summary: Add box-select and issue-move behavior that sends one move order to all selected settlers.
- Rationale: Improves colony-control ergonomics beyond single-pawn ordering.
- Dependencies: Stable per-entity pawn-role classification and deterministic group-order dispatch rules.
- Risks: Selection ordering and per-entity move target fan-out could drift determinism if processing order is not fixed.
- Cut: No formations, squad tactics, or shared-path steering.

### [Diagonal tile movement for settlers]
- Date: 2026-03-04
- Closed-by: Ticket 67
- Category: Scene
- Note: Implemented with deterministic 8-neighbor A* and explicit corner-cut prevention.

### Diagonal tile movement for settlers
- Date: 2026-03-01
- Source: Ticket 64
- Area: Scene
- Summary: Extend settler tile pathfinding from 4-way to diagonal movement with explicit corner-cutting rules.
- Rationale: Produces shorter, more natural routes while preserving deterministic navigation behavior.
- Dependencies: Stable tile-grid A* seam and clear blocked-corner policy decisions.
- Risks: Corner-cutting edge cases can introduce path acceptance inconsistencies if rules are not strict.
- Cut: No navmesh conversion and no crowd steering.

### [Tilemap epoch-triggered lightweight re-path]
- Date: 2026-03-04
- Closed-by: Ticket 67
- Category: Scene
- Note: Implemented tilemap epoch tracking and deterministic stale-path replan behavior.

### Tilemap epoch-triggered lightweight re-path
- Date: 2026-03-01
- Source: Ticket 64
- Area: Scene
- Summary: Add tilemap revision/epoch tracking so settlers can re-path only when relevant tile passability changes.
- Rationale: Avoids stale routes after terrain edits without per-frame path recomputation.
- Dependencies: Tilemap change signaling contract available to gameplay scene state.
- Risks: Incorrect epoch propagation can leave actors on stale paths or trigger unnecessary recomputes.
- Cut: No dynamic obstacle avoidance for moving entities.

### [Settler open-job auto-pick]
- Date: 2026-03-04
- Closed-by: Ticket 67
- Category: Scene
- Note: Implemented deterministic idle-settler auto-pick against open jobs.

### Settler open-job auto-pick
- Date: 2026-03-01
- Source: Ticket 65
- Area: Scene
- Summary: Let idle settlers scan deterministic open jobs and claim nearest unreserved work automatically.
- Rationale: Removes per-settler micro-ordering overhead once first-class jobs exist.
- Dependencies: Stable JobBoard lifecycle, reservation semantics, and deterministic distance tie-break policy.
- Risks: Non-deterministic claim order if actor/job iteration is not explicitly fixed.
- Cut: No global colony scheduler and no cross-map batching.

### [Job priority and reservation timeout]
- Date: 2026-03-04
- Closed-by: Ticket 67
- Category: Scene
- Note: Implemented deterministic priority ordering and reservation-timeout recovery.

### Job priority and reservation timeout
- Date: 2026-03-01
- Source: Ticket 65
- Area: Scene
- Summary: Add numeric job priorities and reservation timeout recovery for stuck claims.
- Rationale: Improves responsiveness and prevents dead reservations in longer simulations.
- Dependencies: Baseline first-class JobBoard assignment and per-job state transitions.
- Risks: Priority starvation and timeout churn if policy is not bounded and deterministic.
- Cut: No UI work planner and no skill/need-based weighting.

## Rendering
### [Hand/tool anchor emission for UseTool visuals]
- Date: 2026-03-04
- Closed-by: Ticket 68
- Category: Rendering
- Note: Implemented renderer-side UseTool attachment anchor consumption with fallback handling.

### Hand/tool anchor emission for UseTool visuals
- Date: 2026-02-28
- Source: Ticket 62.2
- Area: Rendering
- Summary: Emit and consume `hand`/`tool` sprite anchors to support `UseTool` held/tool attachment visuals.
- Rationale: Extend the carry-anchor MVP into practical tool-use presentation without changing simulation authority.
- Dependencies: Stable `UseTool` action visual emission and authored sprite anchor coverage.
- Risks: Anchor naming/authoring drift across sprite sets can cause visual misalignment.
- Cut: No IK, bone rigs, blend trees, or per-pixel deformation systems.

### [Procedural recoil from deterministic seeds]
- Date: 2026-03-04
- Closed-by: Ticket 68
- Category: Rendering
- Note: Implemented deterministic integer/fixed-point recoil behavior tied to stable seed and tick phase.

### Procedural recoil from deterministic seeds
- Date: 2026-02-28
- Source: Ticket 62.3
- Area: Rendering
- Summary: Add renderer-only recoil offsets driven by deterministic per-entity seeds and fixed-tick phase.
- Rationale: Improve action feedback while preserving simulation authority and FPS-independent visual determinism.
- Dependencies: Stable per-entity action-state/action-params visual payload and fixed-tick phase source.
- Risks: Overtuned amplitudes can reduce readability or create perceived jitter near micro-grid boundaries.
- Cut: No simulation-side recoil forces, IK, or skeletal animation systems.

### [Deterministic light flicker polish]
- Date: 2026-03-04
- Closed-by: Ticket 68
- Category: Rendering
- Note: Implemented deterministic flicker polish in renderer without simulation authority changes.

### Deterministic light flicker polish
- Date: 2026-02-28
- Source: Ticket 62.3
- Area: Rendering
- Summary: Add renderer-only light flicker modulation using deterministic per-entity seeds.
- Rationale: Increase scene liveliness with repeatable visual variation that stays independent from simulation logic.
- Dependencies: Existing renderer-only procedural layer and deterministic tick-derived phase.
- Risks: Excessive modulation can become distracting and conflict with clarity on low-end displays.
- Cut: No dynamic lighting pipeline rewrite, no physics-driven effects, and no gameplay visibility logic changes.

## Tools
### [Command Palette multi-step macros]
- Date: 2026-03-04
- Closed-by: Ticket 69
- Category: Tools
- Note: Implemented file-backed command palette macros using the existing console submission path.

### Command Palette multi-step macros
- Date: 2026-02-28
- Source: Microticket 61.3
- Area: Tools
- Summary: Add user-defined command palette macros that execute multiple console commands in sequence.
- Rationale: Improve iteration speed for repeated debug/setup workflows without changing core command routing.
- Dependencies: Finalize command palette preset persistence format and macro execution safety rules.
- Risks: Hidden ordering assumptions could reduce determinism if macros are used without explicit barriers.
- Cut: No macro authoring UI or CI automation in current scope.

### [Overlay pawn role indicator]
- Date: 2026-03-04
- Closed-by: Ticket 69
- Category: Tools
- Note: Implemented typed selected-role snapshot seam with compact overlay role line.

### Overlay pawn role indicator
- Date: 2026-02-28
- Source: Ticket 63
- Area: Tools
- Summary: Show selected actor control role in overlay inspect text (`PlayerPawn`, `Settler`, `Npc`).
- Rationale: Makes control semantics visible during testing without opening code/logs.
- Dependencies: Gameplay exposes stable role value for selected entity in debug snapshot path.
- Risks: Overlay text bloat can reduce readability on low-resolution displays.
- Cut: No new HUD panels or interactive UI widgets.

## Content
No archived entries yet.

## Build/CI
No archived entries yet.
