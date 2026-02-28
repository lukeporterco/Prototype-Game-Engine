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

## Rendering

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

## Physics placeholder

## Audio placeholder

## Scripting seam

## Build/CI
