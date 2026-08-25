# Quint models for pallet-psm

Formal models of the PSM in [Quint](https://quint-lang.org/). The models
encode the pallet's storage invariants and probe them with random simulation
and with bounded model checking.

## Why a model, in addition to try_state

Every `try_state` check bounds `PsmDebt` from above: `reserve >= debt`,
`issuance >= debt`, `debt <= ceiling`. A bug that understates the debt makes
all of those checks pass more easily. The stateful fuzzer ran 48,000 commands
against an injected debt-understatement bug and did not catch it.

The models close that gap with a bidirectional invariant:

```
psmDebt == totalInflow - totalOutflow
```

The pallet cannot check this in `try_state`, because it stores no inflow or
outflow ledger. The model tracks both sides and pins the debt from both
directions.

## Files

| File | Purpose |
| --- | --- |
| `psm.qnt` | Minimal model: one asset, no fees, no decimals. States the problem and the bidirectional invariant. Start here. |
| `psm_buggy.qnt` | Negative control. Same model, but `mint` understates the debt. The invariant must fail. It does, on the first mint. |
| `psm_issuance.qnt` | try_state check 6 (`total_issuance >= total_psm_debt`) under `mul_ceil` fee rounding. |
| `psm_roundtrip.qnt` | Redeem round-trip: the debt shrinks by the round-tripped amount, not by the requested amount. Truncation dust stays with the user. |
| `psm_extended.qnt` | Combined model: three decimal regimes, per-asset fees, donations, asset lifecycle, governance levels, two users. Seven invariants. |

Check numbers in the comments refer to the numbering in `do_try_state`
(see `substrate/frame/psm/src/lib.rs`).

## Running

Random simulation:

```
quint run psm_extended.qnt --invariant=hardInvariant --max-steps=100 --max-samples=10000
```

Bounded model checking (downloads Apalache on first use; needs a JVM):

```
quint verify psm.qnt --invariant=safetyInvariant --max-steps=10
```

The negative control must fail:

```
quint run psm_buggy.qnt --invariant=safetyInvariant
```

## Results

No invariant violation was found in the deployed pallet. The models found
one real defect during development, in a proposal document rather than in
the pallet; the document was corrected. The negative control confirms that
the toolchain reports violations when they exist.
