# Native visual backend boundary

## Status

Implemented; platform-budget and architecture checks pass. Native compilation,
behavior tests and installation packages are validated with the integrating PR.

## Context

The streaming background integration added repeated Windows conditions to view
fields, playback updates and rendering. The platform budget increased from 468
to 520, stopping the 2.1.2 package workflow before it built any installer.

## Evidence

The added conditions selected the same native backend in many places. UI-owned
clocks, generation checks and prepared frames were otherwise platform-independent.
Actual OS differences were window visibility/foreground queries, DWM material
operations and native shader compilation. The system video decoder already has
an explicit unsupported path on other hosts.

## Decision

Keep view state, playback clocks, pending jobs, cancellation and resource
retirement in their existing UI controllers. Compile those controllers through
their feature contracts and consult a shared native backend capability before
creating an animated-media or terminal-effect entity.

Move native window composition into `platform/window_backdrop.rs`, preserving
the existing Windows material-selection and cleanup bodies. Centralize native
window identity/visibility/foreground queries and the shared shader compiler in
`platform/native_visual`. Unsupported hosts expose no active backend and native
compilation returns an explicit error.

Windows-only compiler tests keep their previous host scope, now at the compiler
test module rather than inherited from the entire UI controller. Portable GIF
and frame tests remain available where their implementation compiles.

## Rejected alternatives

- Raise the platform budget or exempt the wallpaper directory.
- Hide the same UI branches behind custom cfg names or preprocessor macros.
- Move the complete UI controller into a platform directory merely to change its
  counting location.
- Change defaults, remove the new effects, or treat compilation as permission to
  activate a source.

## Consequences

Inactive views carry optional controller handles but create no native jobs,
textures or animation timers on an unsupported backend. The supported backend,
saved activation, feature flags and native bytecode format remain unchanged.
The DWM appearance behavior and deferred per-window application remain separate
from UI alpha/layout changes. No dependency or persistent format is added.

## Validation

The same platform checker reports 468 against the unchanged 468 budget. Existing
compiler, decoder, frame and UI tests are retained. Adapter tests reject absent
windows and unsupported native compilation; cross-platform and package results
are recorded with their exact build source.

## Supersedes

The repeated platform conditions in the initial UI integration; not its media
ownership, native lifecycle, activation or memory-budget decisions.

## Revisit when

Another native renderer is qualified for these effects or the public renderer
API provides equivalent window-state and native compilation abstractions.
