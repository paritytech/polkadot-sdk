# HRMP cleanup: short comments

Links are pinned to master `09476c7`. Replace `#PR-A`, `#PR-BC` and `#PR-D` with the PR numbers once opened.

## Tracker (#12718)

Found a few leaks in HRMP open-request cleanup when a para offboards; see #ISSUE, fixes in #PR-A, #PR-BC and #PR-D. A confirmed request is [dropped at enactment without refunds](https://github.com/paritytech/polkadot-sdk/blob/09476c778a37494e60c650372b17126f5a6b85ed/polkadot/runtime/parachains/src/hrmp.rs#L1044-L1078) if its counterparty deregistered that session. [Cleanup](https://github.com/paritytech/polkadot-sdk/blob/09476c778a37494e60c650372b17126f5a6b85ed/polkadot/runtime/parachains/src/hrmp.rs#L972-L988) never decrements the surviving sender's count, refunds recipient deposits system channels never paid, and keeps the outgoing para's own request deposits. Polkadot has 70 DOT stranded on 2004/2012 and 4 leaked counters (5 on Kusama). Worth covering in the HRMP deposit migration.

## A: confirmed request dropped without refund

Confirmed requests that fail `is_valid_para` at enactment are removed without returning either deposit ([hrmp.rs#L1044-L1078](https://github.com/paritytech/polkadot-sdk/blob/09476c778a37494e60c650372b17126f5a6b85ed/polkadot/runtime/parachains/src/hrmp.rs#L1044-L1078)). `deregister` marks the para `Offboarding` immediately, but it only becomes outgoing two sessions later, so `clean_open_channel_requests` never sees the request.

Repro: A requests a channel to B, B accepts, A deregisters in the same session. Two session changes later, B's recipient deposit is still reserved. The mirror case strands A's deposit. `accept_incoming_request_and_offboard` hits this path but doesn't check balances. Fix: #PR-A.

## B: sender's request count leaks

`clean_open_channel_requests` never decrements `HrmpOpenChannelRequestCount` for a sender that isn't outgoing ([hrmp.rs#L972-L988](https://github.com/paritytech/polkadot-sdk/blob/09476c778a37494e60c650372b17126f5a6b85ed/polkadot/runtime/parachains/src/hrmp.rs#L972-L988)). If only the recipient leaves, the sender keeps counting a request that's gone and permanently loses an outbound slot. Cancel and enactment both decrement.

Repro: A requests a channel to B (not accepted), then B deregisters. After three session changes the request is gone but A's count is still 1. On Polkadot, 2008 and 2025 leaked when 2004 was removed (block 32,492,253), and 2000 and 2092 when 2101 was. Fix: #PR-BC.

## C: recipient deposit refunded on system channels

Cleanup refunds `config.hrmp_recipient_deposit` for every confirmed request ([hrmp.rs#L980-L987](https://github.com/paritytech/polkadot-sdk/blob/09476c778a37494e60c650372b17126f5a6b85ed/polkadot/runtime/parachains/src/hrmp.rs#L980-L987)), but accept takes nothing when either side is system ([deposits.rs#L171](https://github.com/paritytech/polkadot-sdk/blob/09476c778a37494e60c650372b17126f5a6b85ed/polkadot/runtime/parachains/src/hrmp/deposits.rs#L171-L173)). The unreserve then eats into deposits backing the system chain's older channels. Polkadot AH still holds 410 DOT of those.

Repro: system para 3 holds an unrelated 50 reserved. Para A requests a channel to 3, then deregisters. 3 accepts in that session. After the next session change, 3 has 35 reserved. Fix: #PR-BC.

## D: outgoing para's own request deposits kept

Cleanup skips the sender deposit when the sender is the outgoing para ([hrmp.rs#L972-L976](https://github.com/paritytech/polkadot-sdk/blob/09476c778a37494e60c650372b17126f5a6b85ed/polkadot/runtime/parachains/src/hrmp.rs#L972-L976)). But `close_hrmp_channel` returns channel deposits to that same account, so request deposits are the only ones never released. The [guide](https://github.com/paritytech/polkadot-sdk/blob/09476c778a37494e60c650372b17126f5a6b85ed/polkadot/roadmap/implementers-guide/src/runtime/hrmp.md#L250) says the same (from #3543); I couldn't find a rationale.

On Polkadot, 2004's removal (block 32,492,253) left 50 DOT reserved for its five pending requests, and 2012 left 20 DOT. Intentional, or should these be refunded like channel deposits? #PR-D does the latter.
