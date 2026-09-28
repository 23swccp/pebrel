# First Copilot review after required CI

## Status

Implemented; activation requires merging the workflow into the default branch.

## Context

Automatic reviews of every push spend review allowance on drafts, failed builds,
and already-reviewed pull requests. Native rulesets target branches, while the
requested policy also depends on PR authorship, readiness, CI, and review history.

## Evidence

GitHub exposes the effective required checks through the branch rules API and
check results for a specific commit. The reviewer-request API accepts
`copilot-pull-request-reviewer[bot]`. Reviews consume the account's applicable
allowance; neither a subscription nor an accepted request proves unlimited usage.

## Decision

The metadata-only [workflow](../../../../.github/workflows/copilot-review.yml)
requests one review for an open, non-draft PR by a human other than the repository
owner, after every required check on its current head succeeds. It reads the
server's effective branch rules rather than maintaining a duplicate check list.
No effective check policy means no automatic review.

Completed CI workflows and draft-to-ready transitions trigger eligibility scans.
A repository-wide concurrency group serializes requests; scanning open PRs also
covers events replaced in GitHub's pending concurrency slot. The dispatch entry
allows evaluating one existing PR with the same guards. Owner and bot PRs are
excluded; an earlier Copilot review on any commit prevents automatic re-review.

A persistent `copilot-review-requested` label is written before requesting review.
If the request fails or its response is ambiguous, leave the claim intact and
require manual inspection before re-requesting. This favors preserving allowance
over automatic retries. Human review, required CI, and merge policy are unchanged.

The workflow does not check out PR code, execute artifacts, interpolate PR text
into commands, approve reviews, merge changes, or use a personal access token.
Its write permissions cover only PR metadata and the review request. Review
instructions point to existing project contracts.

## Rejected alternatives

- Branch-wide Copilot rules: cannot express the entire readiness/author/CI policy.
- Every-push reviews: repeat allowance usage even after an initial review.
- Running PR code under a privileged trigger: unnecessary for metadata decisions.
- In-memory deduplication or retrying uncertain requests: may spend twice.
- A hard-coded list of ten checks: drifts from server-side merge requirements.

## Consequences

A maintainer requests subsequent reviews manually. Removing the marker alone does
not override an existing review. API errors fail the automation without requesting
review or weakening CI. A push can still race the final API call; the workflow
rechecks the head, draft, base, and claim immediately before submitting but cannot
atomically bind GitHub's review request to a SHA. It is supplemental automation,
not an additional required merge check or a billing quota enforcement service.

## Validation

The offline regression executes the actual workflow script with API fixtures:
external successful PR; owner/bot/draft/closed exclusions; missing/failed/skipped
checks; newer pending attempts; wrong app/head; changed PR state; review history;
persisted claims; policy errors; and ambiguous request failure. The required lint
job runs this contract before scheduling native runners.

## Supersedes

None.

## Revisit when

GitHub supports equivalent native conditions or an atomic review-request API,
required checks move to legacy commit statuses, or repository ownership changes.
