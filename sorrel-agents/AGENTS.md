# Agent instructions for sorrel-agents

## What this module is

The planned agent control plane for Sorrel: agent lanes, task assignment,
file/symbol claims, and policy overlays (org/repo/path `AGENTS` rules), built on
top of Core lanes/stacks and the Core policy model.

**Status: minimal control plane shipped.** Register agents, advisory path
claims, and an active-work view. Instruction overlays and Hub write UI remain
ahead (see root `ROADMAP.md`).

## Intended boundary

- Agents receive only allowed instructions and secret *handles*; permissions are
  evaluated by Core, not redefined here.
- File/symbol claims are advisory unless policy requires blocking.

## Common checks

To be defined when implementation starts (match the chosen stack: Rust ->
`cargo test`/`clippy`/`fmt`; Node -> `npm test`/`lint`).

## Workflow

- Keep changes scoped to this repository.
- Prefer small, reviewable commits.
- Do not commit secrets.
- Coordinate shared contracts through `sorrel-protocol`.
