# Terminal WGSL effects and frame inputs

## Status

Product integration in progress. The source compiler and settings contracts have
targeted checks; full product and interaction acceptance are still separate.

## Context

Background-only effects cannot inspect text or cursor state. The renderer now has
a scoped post-processing operation, but a window-global cursor snapshot would mix
split panes. Effects must also remain independent from the selected background.

## Evidence

TerminalElement already captures the visible grid, OSC color overrides, palette
and painted cursor. It paints completion/IME overlays separately. The native API
requires exact physical dimensions and retains resources until GPU completion.

## Decision

- Put activation, source path and animation mode in the zero-dependency settings
  authority. Source selection disables activation; neither a path nor animation
  preference authorizes execution. Background preferences are untouched.
- Give each terminal view its own controller, native owner and cursor history.
  Reuse the existing global compiler permit and GPU byte budget. Hidden terminal
  views release their GPU owner rather than accumulating textures across tabs.
- Capture data from the same snapshot used to paint. Process text, inline content
  and the cursor before completion popups, IME preedit and application overlays.
- Use a WGSL prelude with a versioned, 16-byte-block frame layout and one read-only
  surface texture. Expose time/delta/frame, focus, cursor history and theme/palette
  data. Fragment declaration order is pass order; one through eight passes share
  the renderer's two physical-size intermediate textures.
- Provide clamped pixel loads and bilinear `sample_surface` in the WGSL interface.
  The pinned Naga HLSL sampler path assumes descriptor heaps; keeping this one-layer
  surface sampling in WGSL avoids a D3D12 binding-heap or source-language adapter.
- Keep native bytecode compilation in one source implementation shared with the
  background compiler. Read files and compile only on owned background work.
- Handle cancellation, stale source/native receipts, visibility and reload without
  GPU work on the UI thread. Stop recurring animation while inactive/reduced-motion;
  the content-change mode has no recurring timer. Timestamp observations remain
  monotonic when a later content update is rendered.

## Rejected alternatives

- Reuse one global cursor for unrelated panes.
- Turn off a video/background when enabling terminal post-processing.
- Filter menus or IME by applying the effect at whole-window presentation.
- Reduce text resolution to reuse the smaller background target.
- Add another user-facing shader language or silently accept extra resource bindings.

## Consequences

The frame ABI and source limits are documented with the included effect examples.
Uniform packing is explicit little-endian data, including integer flags. Background
and terminal effects share resource pressure; an oversized surface produces a local
failure rather than an unbounded allocation. Native backend support still limits
activation; this change does not establish other-platform or full parity acceptance.

## Validation

Settings round-trip/default/activation cases are part of the passing settings suite.
The actual compiler verifies frame layout, palette indexing, sampling helpers and
ordered fragment compilation to DXBC, plus invalid binding/stage/size rejection.
Frame packing has a focused regression. Full product, real controls, source switching,
split isolation, pause/resume and long-running resource behavior remain to be run.

## Supersedes

None. This adds a separate post-processing channel, retaining background semantics.

## Revisit when

A further native backend, additional source composition or measured runtime costs
require an ABI, sampling or admission change.
