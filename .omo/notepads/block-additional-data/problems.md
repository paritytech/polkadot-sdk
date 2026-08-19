# Unresolved blockers - block-additional-data

(none yet)

## [TODO-10] BABE/AURA self-import gap — ACKNOWLEDGED, OUT OF SCOPE

**Status:** Known, accepted limitation. NOT a bug to fix.

**What:** Plain (non-cumulus) consensus engines (BABE, AURA, manual-seal) do not copy
`Proposal::additional_data` into `BlockImportParams::additional_data` when they self-import
their own freshly-authored block. As a result, a validator/collator on a plain Substrate chain
that authors a block will not have that block's `additional_data` persisted locally via the
self-import path.

**Why it is OK:**
- Cumulus chains have their own self-import path already wired (todo 11, already complete).
- All peers that receive the block via the network sync path get correct `additional_data`
  (todo 12, already complete — `BlockImportParams::additional_data` is populated from the
  `ADDITIONAL_DATA` block attribute during sync).
- The feature's primary use-case (parachains/cumulus) is fully covered.

**What would fix it:** Each consensus engine (BABE, AURA, manual-seal, etc.) would need to
copy `proposal.additional_data` into `BlockImportParams::additional_data` in its
`import_block` or equivalent self-import call site. This is a separate, follow-up todo and
is explicitly NOT part of Wave 3.
