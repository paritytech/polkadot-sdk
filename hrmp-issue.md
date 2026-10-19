# Issue

**Title:** HRMP: open channel request cleanup leaks deposits and request counts when a para offboards

**Labels:** `I2-bug`, `T8-polkadot`

---

When a para deregisters while open channel requests involving it are pending, the relay chain's HRMP session-change handling leaves deposits reserved and request counters incremented. The implementers' guide ([Session Change](https://github.com/paritytech/polkadot-sdk/blob/09476c778a37494e60c650372b17126f5a6b85ed/polkadot/roadmap/implementers-guide/src/runtime/hrmp.md#L241-L265)) specifies the same behaviour.

**A.** A confirmed request whose sender or recipient is offboarding is [dropped at enactment](https://github.com/paritytech/polkadot-sdk/blob/09476c778a37494e60c650372b17126f5a6b85ed/polkadot/runtime/parachains/src/hrmp.rs#L1044-L1078) without returning either deposit.
Repro: A requests a channel to B, B accepts, and A deregisters in the same session. Two session changes later, both deposits are still reserved.

**B.** [`clean_open_channel_requests`](https://github.com/paritytech/polkadot-sdk/blob/09476c778a37494e60c650372b17126f5a6b85ed/polkadot/runtime/parachains/src/hrmp.rs#L972-L988) doesn't decrement the sender's `HrmpOpenChannelRequestCount` when only the recipient leaves. The sender loses an outbound slot for good.

**C.** The same cleanup unreserves `hrmp_recipient_deposit` for confirmed requests with or amongst the system, where `accept_open_channel` takes none.

**D.** The same cleanup keeps the outgoing para's own request deposits reserved, while `close_hrmp_channel` returns its channel deposits.

**On mainnet:**
- B: leaked counters on Polkadot 2000, 2008, 2025 and 2092, and on five Kusama paras.
- D: 70 DOT stranded on Polkadot, from 2004 (removed at block 32,492,253) and 2012.

The fixes don't touch this existing state; it needs handling in the HRMP deposit stage of #13481.

Fixes: #PR-A (A), #PR-BC (B, C), #PR-D (D). D changes specified behaviour, so it needs a decision.

Related: #12718, #13475.
