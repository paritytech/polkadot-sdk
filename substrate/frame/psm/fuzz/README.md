# pallet-psm fuzz targets

Two complementary fuzzers for the PSM pallet's mint/redeem/ceiling/governance
state space. Both exercise all 9 dispatchables and validate `do_try_state()`
after each block (libFuzzer) or each command (stateful).

## `psm` — coverage-guided (libFuzzer)

Generates random byte sequences, parsed into `Op` hints by `Arbitrary`.
Hints are resolved to concrete calls at execution time using current pallet
state (debt, ceilings, balances). Runs indefinitely; corpus accumulates in
`corpus/psm/`. Crashes (invariant violations) go to `artifacts/psm/`.

Run:
	cargo +nightly fuzz run psm -- -max_len=4096

Reproduce crash:
	cargo +nightly fuzz run psm artifacts/psm/<crash-file>

Architecture: `Op` enum → `dispatch_op()` reads storage via `fuzz_helpers`
→ dispatches → `do_try_state()`. No storage access during `Arbitrary` parse.

## `psm_stateful` — state-aware property tester

Generates commands by reading actual pallet state before each decision.
Uses `rand::StdRng` with a fixed seed for reproducibility. Biases toward
interesting scenarios: ceiling violations, fee extremes, weight redistribution
with existing debt, circuit breaker toggling.

Run:
	cargo run --bin psm_stateful -- <seed> <max_commands>
	cargo run --bin psm_stateful -- 42 10000

Deterministic: same seed produces the same sequence. Logs every command
with the state snapshot that led to it. Panics on invariant violation
with full reproduction trace.

## Stateful fuzzer output format

Each line printed by `psm_stateful` follows this format:

```
[N] Operation   dispatch=OK/ERR  invariant=OK  PRE  debt=X/Y issuance=X/Y reserve=X  POST debt=X/Y issuance=X/Y reserve=X
```

- `debt=X/Y`: X is total psmDebt across all assets (internal pUSD units, 6 decimals). Y is the
  current debt ceiling, recomputed as `maxPsmDebtPermill * MaximumIssuance / 1_000_000`. Y changes
  whenever `SetMaxPsmDebt` fires.
- `issuance=X/Y`: X is total pUSD supply in circulation. Y is `MaximumIssuance`, fixed at genesis.
- `reserve=X`: total external assets held by the PSM, in pUSD-equivalent internal units.
- X in `debt` and X in `issuance` are always equal: every pUSD in existence was minted through
  the PSM, so outstanding debt equals outstanding supply 1:1.

## `corpus_gen` — seed generator

Generates hand-crafted corpus entries for the `psm` libFuzzer target and
writes them to `corpus/psm/`. Targets error paths not reachable by random
mutation: `AssetAlreadyApproved` and `AmountTooSmallAfterConversion`.

Run:
	cargo run --bin corpus_gen -- fuzz/corpus/psm

The `Op` selector byte encoding and block framing layout are documented
inline. Re-run after any change to the `Op` enum or `Arbitrary` impl.

## When to use which

- **libFuzzer**: long-running background campaigns, coverage tracking, corpus
  minimization. Good for finding unexpected code paths. Limited by entropy
  budget — each `Arbitrary` parse consumes 4+ bytes per op, so corpus entries
  stay small and multi-op sequences are rare.

- **Stateful**: targeted campaigns, reproducing specific scenarios, exploring
  deep state interactions (governance→debt→redeem chains). Generates long
  semantically rich sequences by design. No coverage feedback — relies on
  domain knowledge for interestingness.

## Genesis

Both use the same genesis: 10 accounts, 5 pre-created external assets
(USDC, USDT, DAI, USDP, FRAX at IDs 2–6), PSM configured with USDC 60% /
USDT 40%, MaxPsmDebtOfTotal 50%, MaximumIssuance 20M pUSD. Assets 4–6
are unapproved — available for `add_external_asset` during fuzzing.

## Mock access

The pallet's internal methods and storage are `pub(crate)`. The fuzz crate
accesses them via `pallet_psm::mock::fuzz_helpers`, a module gated behind
`#[cfg(feature = "fuzzing")]` that exposes `psm_debt()`, `max_psm_debt()`,
`total_psm_debt()`, `do_try_state()`, `approved_assets()`, etc.

## `do_try_state` checks

Checks 4 (global ceiling) and 5 (per-asset ceiling) are warnings, not hard
invariants — governance can transiently create these states. The fuzzers
exercise them freely. All other checks (reserve ≥ debt, debt sum integrity,
no orphan debt, etc.) are hard and will cause a panic.
