# PR descriptions

Replace `#ISSUE` (see `hrmp-issue.md`), `#PR-A`, `#PR-BC` and `#PR-D` once opened. Split from #13478.

---

## PR A: `hrmp-enactment-refund`

**Title:** hrmp: give back both deposits when a confirmed request cannot be enacted

A confirmed open channel request whose sender or recipient is no longer valid at the next session change is removed without returning either deposit. `deregister` marks a para `Offboarding` immediately, but it only becomes outgoing two sessions later, so `clean_open_channel_requests` never sees the request. This change gives both deposits back through the deposit provider, as `close_hrmp_channel` does.

Tests: `accept_incoming_request_and_offboard_refunds_recipient_deposit` and `accept_incoming_request_and_recipient_offboards_refunds_sender_deposit`. Both fail on master.

Doesn't touch deposits already stranded on-chain. Part of #ISSUE (tracked in #12718). Related: #PR-BC, #PR-D.

---

## PR B + C: `hrmp-cleanup-request-count`

**Title:** hrmp: decrement sender request count and skip system-channel recipient refund in cleanup

Two fixes in `clean_open_channel_requests`:

- When only the recipient is outgoing, the sender's `HrmpOpenChannelRequestCount` was never decremented, so the request kept taking an outbound slot. This has happened on Polkadot (2000, 2008, 2025, 2092) and Kusama.
- For confirmed requests with or amongst the system, `hrmp_recipient_deposit` was unreserved although `accept_open_channel` takes none.

The cleanup benchmark now uses confirmed requests, the new worst case; weights need `/cmd bench`.

Tests: `pending_request_to_offboarded_recipient_releases_sender_request_count`, `offboarding_sender_of_system_channel_does_not_unreserve_system_recipient_funds`, and the guard `cleanup_at_a_deposit_config_change_refunds_what_was_reserved`.

Existing leaked counters need a recount, e.g. in the HRMP deposit stage of #13481. Part of #ISSUE (tracked in #12718). Related: #PR-A, #PR-D.

---

## PR D: `hrmp-refund-outgoing-request-deposits`

**Title:** hrmp: give back an offboarding para's own open request deposits on cleanup

When a para is removed, `close_hrmp_channel` returns its channel deposits, but `clean_open_channel_requests` keeps the deposits of its pending requests reserved, with nothing left to release them. On Polkadot this left 50 DOT on 2004 and 20 DOT on 2012. This PR gives back both deposits of every removed request, including the outgoing para's.

This changes behaviour the implementers' guide specifies (from #3543), so it needs a maintainer decision; the guide is updated here.

Tests: `offboarding_sender_gets_its_pending_request_deposit_back` and `offboarding_recipient_gets_its_accepted_request_deposit_back`.

Edits the same lines as #PR-BC; whichever merges second needs a small rebase. The stranded 70 DOT is for the HRMP deposit stage of #13481. Part of #ISSUE (tracked in #12718).

---

## Closing comment for #13478

Split into #PR-A, #PR-BC and #PR-D, one per fix, so they can be reviewed and merged independently. Closing this one.
