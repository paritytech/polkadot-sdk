# Quint models for pallet-psm

Models of the PSM in [Quint](https://quint-lang.org/). They encode the
pallet's storage invariants and probe them with random simulation and with
bounded model checking.

## Why a model, in addition to try_state

Every hard `try_state` check bounds `PsmDebt` from above: `reserve >= debt`,
`issuance >= debt`, `debt <= ceiling`. A bug that understates the debt moves
all of them further from their boundary, so none of them can see it. The
stateful fuzzer ran 48,000 commands against an injected understatement and
did not catch it.

The models add a bidirectional invariant:

```
psmDebt == totalInflow - totalOutflow
```

The pallet does not check this today, because it keeps no inflow or outflow
ledger: two monotone counters written on mint and redeem would make it a
`try_state` check like any other. The model keeps those counters, so the
model-based tests can hold the pallet to the invariant while the pallet's own
storage stays as it is. If the counters are later added to the pallet, the
check moves on-chain and the model keeps agreeing with it.

## Files

| File | Modules | Purpose |
| --- | --- | --- |
| `psm.qnt` | `psm`, `psm_correct`, `psm_buggy` | One asset, no fees, no decimals. `psm` takes `understateDebtOnMint`; `psm_correct` instantiates it false, `psm_buggy` true. Start here. |
| `psm_extended.qnt` | `psm_extended` | Three decimal regimes, per-asset fees and ceilings, donations, asset lifecycle, governance levels, two users. |

`psm_extended` is a second abstraction level rather than an extension of
`psm`: its state is per-asset and per-user maps where `psm` has scalars, so it
shares no declarations with it.

Both models cover a single PSM instance. `PsmDebt` is a double map in the
pallet, keyed by `(internal_asset, external_asset)`, but every invariant
modelled here is per-instance, so the instance dimension adds no reachable
behaviour.

Check numbers in the comments refer to the numbering in `do_try_state` (see
`substrate/frame/psm/src/lib.rs`). Of its seventeen checks, five are
advisory: they warn and return `Ok`, because a permitted call creates the
state they report. Checks 10 and 17 follow a ceiling change by governance,
checks 2 and 7 a metadata change by an asset owner. The models keep the same
split, so advisory properties stay out of `safetyInvariant` and
`hardInvariant`.

## Decimals

`create_psm` and `add_external_asset` record the decimals of the internal and
external assets. Swaps use those recorded values and never read live
metadata, so an owner who changes metadata afterwards does not affect the
pallet's arithmetic. `psm_extended` models this: `recordedDecimals` is
constant, `liveDecimals` is state that `setLiveDecimals` moves, and
`hardInvariant` holds across that drift. `decimalsMatchLiveMetadata` is the
advisory counterpart; simulation violates it within a few steps, which is why
the pallet warns rather than fails.

## Running

Random simulation:

```
quint run psm_extended.qnt --invariant=hardInvariant --max-steps=60 --max-samples=3000
```

Bounded model checking (downloads Apalache on first use; needs a JVM):

```
quint verify psm.qnt --main=psm_correct --invariant=safetyInvariant --max-steps=10
```

The negative control must fail:

```
quint verify psm.qnt --main=psm_buggy --invariant=safetyInvariant --max-steps=10
```

## Results

No invariant violation was found in the pallet as merged.

| Run | Result |
| --- | --- |
| `psm_correct`, Apalache, 10 steps | no violation, 32s |
| `psm_extended` `hardInvariant`, 3000 traces of 60 steps | no violation |
| `psm_buggy`, Apalache, 10 steps | violation, 5s |
| `psm_buggy`, simulation | violation on the first mint |

The negative control confirms that the toolchain reports violations when they
exist. The models found one real defect during development, in a proposal
document rather than in the pallet; the document was corrected.
