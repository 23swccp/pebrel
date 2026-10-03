# Streaming emoji cell allocation

## Status
Proposed fix for Issue #403; remote validation pending.

## Context
Single-codepoint input width splits ZWJ, modifier and regional-indicator emoji.
The report supplies seven exact sequences and contrasts single-codepoint emoji.

## Evidence
`Term::input` previously placed every positive-width codepoint independently.
`Cell::extra`, copying and `RenderSnapshot` already retain a head plus trailing
codepoints; GPUI sends that complete cell string to its existing text shaper.
The grid allocation prevents composition before font support can be evaluated.

## Decision
Move character placement into `term/input.rs`, a real input responsibility rather
than a file-budget split. Reuse the locked unicode-segmentation 1.13.3 UAX #29
implementation and unicode-properties 0.1.4 Emoji property (emoji feature only)
as direct core dependencies. They add no renderer dependency or new lock package.
Unicode-width 0.2.2 remains the width authority for individual bases and bounded
modifier/RI/presentation pairs. Emoji ZWJ continuations occupy two columns;
non-emoji positive-width allocation retains the existing contract.

A GraphemeCursor with open-ended length consumes each UTF-8 codepoint and defers
its final boundary using NextChunk. PreContext is supplied from the owning cell
only when requested; its cache then grows incrementally. Ordinary ASCII letters
and CJK have no cluster string allocation. Combining marks do not trigger whole
cluster width scans. Emoji presentation pairs and standard keycaps use a bounded
UTF-8 buffer; already composed emoji remain two columns.

Consecutive ownership ends at actual cursor/grid edits, alternate-buffer changes
and resize. No-op resize, SGR, synchronized output and parser chunk boundaries do
not end it. Width promotion/shrink updates the head/spacer and pending-wrap cursor
state; promotion at the last column relocates the complete cell through normal
wrapping. The leading spacer copy reads the following row and its full text.

## Rejected alternatives
A hand-maintained emoji table would drift. Per-codepoint reconstruction/width
scans make long untrusted combining output quadratic. Unconditionally merging
all graphemes into two columns changes Indic and other non-emoji width contracts.
A font substitution does not repair terminal cell allocation.

## Consequences
Existing cell extra storage carries positive-width emoji continuations too.
SGR inside one emoji retains the first cell's style. Line-wrap-disabled edge
promotion retains clipped text without creating an out-of-bounds spacer.
This fixes model/snapshot input to the shaper; actual Windows glyph/font coverage
remains a separate native acceptance requirement.

## Validation
Active parser tests cover the original seven sequences and controls across UTF-8
chunks, copy/spacer selection, snapshots, selectors/last-column wrap/reflow,
SGR/sync, edits/DECALN/scroll/cursor resets and long combining output. Remote
representative ASCII/CJK/emoji/long-mark timing and allocation comparison against
the exact main baseline is pending. No local builds or tests were executed.

## Supersedes
None. PR #431's ignored test-only reproducer is evidence, not a production fix.

## Revisit when
Unicode library versions, non-emoji grapheme allocation or font backend support
change. Reported performance applies only to the measured workload/build/runner.
