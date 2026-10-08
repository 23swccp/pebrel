# Windows portable ZIP update handoff

## Status

Proposed for review. Windows x64 native debug same-version rehearsals and
PowerShell 5.1 fixtures pass; mouse/keyboard controls and visual acceptance
remain unverified because the desktop automation connection is unavailable.

## Context

Windows discovery selected setup.exe before installation ownership was checked.
A portable copy could download and verify setup, only to fail when the handoff
required unins000.exe. The agreed change reuses the updater and preserves files
the user added to the portable installation directory.

## Evidence

- [Asset selection](../../../nebula_app/src/update_check/assets.rs) previously
  offered only Windows installers.
- [Native ownership](../../../nebula_app/src/platform/update_installation.rs)
  already used the uninstall marker; a directory name does not establish ownership.
- [Windows packaging](../../../scripts/package-release.ps1) emits a ZIP with
  pebrel.exe at its root, with runtime, docs, skills and licenses beneath it.
- [Helper fixtures](../../../scripts/tests/test_update_handoff.ps1) run the actual
  embedded helper source using Windows PowerShell 5.1 and small native processes.

## Decision

External distribution ownership still takes precedence. For direct Windows
copies, the existing uninstall-marker rule selects setup or the exact same-arch
official ZIP before downloading. Invalid/unreadable markers fail closed. The
handoff revalidates the selected asset, so a cached installer cannot be applied
to a portable copy after its ownership changes.

Streaming, SHA-256 verification, download cancellation, cache state, workspace
snapshots, exact process identity and prepare/commit authority remain shared.
Schema 1 gains an optional portable payload descriptor; old restore tickets
remain readable. Existing ZIP and SHA dependencies handle staging on the
background preparation path before any window exits.

ZIP inspection rejects traversal, Windows path aliases, reserved updater/channel
files, case duplicates, file/directory collisions and non-regular entries. Limits
are 256 entries, 512 MiB per file and 1 GiB expanded data. Staging reads the full
entries (including ZIP CRC checks) and records file sizes and SHA-256 values.
Rust locks the ZIP against writes/deletion while reverifying and staging it, so
the manifest describes files from the verified archive. The helper then locks
and rechecks the archive and staged files before readiness,
and rejects reparse points along affected source/target paths.

After commit and participant exit, the helper checks helper-file locks and backs
up every affected old file before replacing any. It copies only ZIP-listed files
to the original path and checks their hashes and the resulting executable version.
Ordinary failures restore all attempted files in reverse order, delete newly added
files and reopen the verified old executable with its workspace restore ticket.
A failed rollback is reported and does not launch a partially restored package.

Backups live in .pebrel-update-<transaction> beneath the installation. After the
new application acknowledges workspace restoration, background cleanup removes
successful staging data and downloaded ZIPs, and retains the newest successful
backup for this installation/configuration. It prunes older acknowledged-success
backups only. Failed and unfinished transactions retain their artifacts; plans,
results, journals and workspace snapshots remain for diagnosis. Startup retries
deferred cleanup on the existing background hydration path.

Cleanup holds the installation lock, checks the exact executable/configuration
and success/restoration identities, and validates the whole reserved subtree
against its package manifest before deletion. Reparse points, unexpected files
and concurrent download locks defer cleanup. ZIPs referenced by failed/pending
transactions, scheduled updates or newer download records are retained.

Windows free-space checks run before streaming a new download and before portable
extraction. Download size uses release/HTTP metadata, with the existing 512 MiB
limit as the conservative fallback. ZIP entry sizes budget staging; existing
target file sizes plus the new file sizes budget backup/replacement. Reservations
on the same volume GUID are added; separate volumes are checked independently,
with one 32 MiB metadata/headroom allowance per volume. The check uses bytes
available to the calling user, including quotas. It is repeated after extraction
for the remaining installation requirement. Query failures stop preparation and
shortages show localized required/available amounts before file replacement.
This is a precheck, not a reservation against other processes consuming space.
Files absent from the new ZIP are retained: there is no authoritative historical
ownership manifest that can distinguish removed package files from user files.

## Rejected alternatives

- A second downloader or network-capable PowerShell updater would duplicate
  proxy, official URL, digest, cancellation and progress behavior.
- Whole-directory replacement would hide or discard the user's extra files.
- Per-file old/new comparisons add complexity without avoiding the need to exit
  when pebrel.exe changes. The downloaded ZIP already contains the full payload.
- Treating all old runtime/docs files as package-owned cannot safely distinguish
  user additions, so automatic obsolete-file removal is deferred.

## Consequences

The helper remains embedded through include_bytes! and explicitly uses the OS
Windows PowerShell executable; it does not depend on the user's default pwsh.
Repeated JSON writes use a real temporary backup path because PowerShell 5.1
binds a null File.Replace backup argument as an empty, invalid path.

Native rehearsal found that an inherited module search environment could make
Get-FileHash unavailable. The helper now hashes files with the same .NET SHA-256
primitive used for its archive and staging checks. Process enumeration also
races with CLI exit and early loader startup: MainModule.FileName may be empty.
QueryFullProcessImageName against the process handle supplies the authoritative
image path; confirmed exits are ignored, but unidentified live processes still
stop installation. Busy-file errors use stable path/recovery details rather than
unreadable localized method-invocation text observed in the native helper.

Replacement is transactional for handled failures, not atomic across the entire
package. A power loss or forcibly killed helper can leave a partial update;
backups and the journal are retained but automatic crash replay is not implemented.
The latest successful backup and failed transaction artifacts still consume disk
space deliberately. Cleanup uses the existing restoration acknowledgement; it
does not introduce the separately deferred full-window startup health handshake.

## Validation

On Windows x64, the PowerShell 5.1 suite passed 22 scenarios. A temporary
independent harness compiled the production ZIP, ownership, cleanup, capacity and
asset-selection modules and passed 24 tests; it does not substitute for product
integration tests. The independent i18n contract passed 24 tests (one manual
benchmark ignored). The GPUI product/tests cargo check, architecture check
against 244d6fe1 and diff whitespace check passed. Changed Rust files passed
targeted formatting; full-workspace formatting still reports existing CRLF files
against the repository's Unix newline setting.

The PowerShell fixture suite covers original installer flows and portable success,
version-mismatch rollback, occupied helpers, staging tampering, traversal and
cancellation. It verifies retained user files, old backups, newly added-file
rollback, configuration-directory propagation and workspace restore tickets.
Rust tests cover marker classification, exact package selection, ZIP structure,
staged bytes/digests, dangerous paths, collisions, truncation and file-count limits.
These fixtures do not establish a full GPUI product upgrade, ARM64 execution,
visual acceptance, or recovery after power loss.

Native product testing subsequently passed eight isolated same-version scenarios
using the complete 17-file ZIP payload: two consecutive same-volume updates,
two consecutive C-config/D-program updates, interrupted download, bad SHA-256,
ZIP traversal, occupied helper, post-replacement version-mismatch rollback and
cached-package tampering. Runtime CLI verified two tabs, three panes, split ratio,
focus and directories. Configuration bytes were compared across successful
updates; user-added files remained intact. Success cleanup retained one backup.
The actual product updater tests passed 32 cases; discovery tests passed 22
(one public-network case ignored). Capacity shortages used controlled budget
tests rather than filling a real disk. These native rehearsals use automatic
download and scheduled-on-launch installation; immediate-button clicks, visual
acceptance, formal release packages and ARM64 execution remain separate checks.

## Supersedes

None. The macOS bundle handoff and installer-managed Windows flow remain owned
by their existing adapters.

## Revisit when

The package has a reviewed historical ownership manifest, failed-artifact
retention limits, or a requirement for automatic recovery after interruption.
