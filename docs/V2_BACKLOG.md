# V2 Backlog (Canonical)

This is the single canonical backlog for deferred V2 ideas captured during ticket work.

How to use this doc:
- When a ticket includes ROADMAP `V2` bullets, append each bullet as a separate entry under exactly one module section below.
- Use the strict template exactly as written (field names, order, and headings).
- If a ticket says `V2: None`, do not change this file.

## Strict Entry Template (Copy/Paste)
```md
### [Short title]
- Date: YYYY-MM-DD
- Source: <ticket id or doc link>
- Area: <Core|App/Loop|Scene|Rendering|Assets|Input|Tools|Physics placeholder|Audio placeholder|Scripting seam|Build/CI>
- Summary: <one concise statement of the deferred V2 change>
- Rationale: <why this should exist>
- Dependencies: <none or required prior work>
- Risks: <none or key risk(s)>
- Cut: <what is explicitly out of scope right now>
```

## Core

## App/Loop

## Scene

### Action state interact/carry emission
- Date: 2026-02-28
- Source: Ticket 62.1
- Area: Scene
- Summary: Emit `Interact` and `Carry` action visuals for one interactable workflow.
- Rationale: Extend the MVP Idle/Walk visual contract so renderer can reflect core interaction intent.
- Dependencies: Stable interaction-state to action-state mapping for player workflow.
- Risks: Visual-state drift if action emission is not aligned with deterministic gameplay tick boundaries.
- Cut: No blend trees, IK, ragdolls, or physics-driven animation.

### Visual sandbox second interactable lane
- Date: 2026-02-28
- Source: Ticket 62.4
- Area: Scene
- Summary: Add a second deterministic interactable lane in `visual_sandbox` with role-stable payload semantics.
- Rationale: Expand demo coverage for interaction-state visuals without changing command schema or scenario id.
- Dependencies: Stable visual-sandbox spawn ordering and interaction target resolution by save-id.
- Risks: Scenario clutter can reduce readability if overlap/layout is not kept intentional.
- Cut: No new console commands and no scenario payload schema changes.

### Visual sandbox deterministic hit-kick trigger
- Date: 2026-02-28
- Source: Ticket 62.4
- Area: Scene
- Summary: Add a deterministic demo trigger path that forces `Hit` action visual state in `visual_sandbox`.
- Rationale: Ensure hit-kick polish can be validated visually in a repeatable scene.
- Dependencies: Existing renderer-side `Hit` procedural support and deterministic sandbox state rules.
- Risks: Trigger timing may conflict with ongoing interaction states if priority rules are unclear.
- Cut: No combat-system redesign and no non-deterministic/random trigger logic.

### Multi-select settlers move order
- Date: 2026-02-28
- Source: Ticket 63
- Area: Scene
- Summary: Add box-select and issue-move behavior that sends one move order to all selected settlers.
- Rationale: Improves colony-control ergonomics beyond single-pawn ordering.
- Dependencies: Stable per-entity pawn-role classification and deterministic group-order dispatch rules.
- Risks: Selection ordering and per-entity move target fan-out could drift determinism if processing order is not fixed.
- Cut: No formations, squad tactics, or shared-path steering.

### Diagonal tile movement for settlers
- Date: 2026-03-01
- Source: Ticket 64
- Area: Scene
- Summary: Extend settler tile pathfinding from 4-way to diagonal movement with explicit corner-cutting rules.
- Rationale: Produces shorter, more natural routes while preserving deterministic navigation behavior.
- Dependencies: Stable tile-grid A* seam and clear blocked-corner policy decisions.
- Risks: Corner-cutting edge cases can introduce path acceptance inconsistencies if rules are not strict.
- Cut: No navmesh conversion and no crowd steering.

### Tilemap epoch-triggered lightweight re-path
- Date: 2026-03-01
- Source: Ticket 64
- Area: Scene
- Summary: Add tilemap revision/epoch tracking so settlers can re-path only when relevant tile passability changes.
- Rationale: Avoids stale routes after terrain edits without per-frame path recomputation.
- Dependencies: Tilemap change signaling contract available to gameplay scene state.
- Risks: Incorrect epoch propagation can leave actors on stale paths or trigger unnecessary recomputes.
- Cut: No dynamic obstacle avoidance for moving entities.

## Rendering

### Hand/tool anchor emission for UseTool visuals
- Date: 2026-02-28
- Source: Ticket 62.2
- Area: Rendering
- Summary: Emit and consume `hand`/`tool` sprite anchors to support `UseTool` held/tool attachment visuals.
- Rationale: Extend the carry-anchor MVP into practical tool-use presentation without changing simulation authority.
- Dependencies: Stable `UseTool` action visual emission and authored sprite anchor coverage.
- Risks: Anchor naming/authoring drift across sprite sets can cause visual misalignment.
- Cut: No IK, bone rigs, blend trees, or per-pixel deformation systems.

### Procedural recoil from deterministic seeds
- Date: 2026-02-28
- Source: Ticket 62.3
- Area: Rendering
- Summary: Add renderer-only recoil offsets driven by deterministic per-entity seeds and fixed-tick phase.
- Rationale: Improve action feedback while preserving simulation authority and FPS-independent visual determinism.
- Dependencies: Stable per-entity action-state/action-params visual payload and fixed-tick phase source.
- Risks: Overtuned amplitudes can reduce readability or create perceived jitter near micro-grid boundaries.
- Cut: No simulation-side recoil forces, IK, or skeletal animation systems.

### Deterministic light flicker polish
- Date: 2026-02-28
- Source: Ticket 62.3
- Area: Rendering
- Summary: Add renderer-only light flicker modulation using deterministic per-entity seeds.
- Rationale: Increase scene liveliness with repeatable visual variation that stays independent from simulation logic.
- Dependencies: Existing renderer-only procedural layer and deterministic tick-derived phase.
- Risks: Excessive modulation can become distracting and conflict with clarity on low-end displays.
- Cut: No dynamic lighting pipeline rewrite, no physics-driven effects, and no gameplay visibility logic changes.

## Assets

## Input

## Tools

### Command Palette multi-step macros
- Date: 2026-02-28
- Source: Microticket 61.3
- Area: Tools
- Summary: Add user-defined command palette macros that execute multiple console commands in sequence.
- Rationale: Improve iteration speed for repeated debug/setup workflows without changing core command routing.
- Dependencies: Finalize command palette preset persistence format and macro execution safety rules.
- Risks: Hidden ordering assumptions could reduce determinism if macros are used without explicit barriers.
- Cut: No macro authoring UI or CI automation in current scope.

### Overlay pawn role indicator
- Date: 2026-02-28
- Source: Ticket 63
- Area: Tools
- Summary: Show selected actor control role in overlay inspect text (`PlayerPawn`, `Settler`, `Npc`).
- Rationale: Makes control semantics visible during testing without opening code/logs.
- Dependencies: Gameplay exposes stable role value for selected entity in debug snapshot path.
- Risks: Overlay text bloat can reduce readability on low-resolution displays.
- Cut: No new HUD panels or interactive UI widgets.

## Physics placeholder

## Audio placeholder

## Scripting seam

## Build/CI
