# Quint models for pallet-psm

Models of the PSM in [Quint](https://quint-lang.org/). They encode the
pallet's storage invariants and probe them with random simulation and with
bounded model checking.

## Why a model, in addition to try_state

[Check 6](https://github.com/paritytech/polkadot-sdk/blob/d2e08992098ac61e13117d61420903e1e076d8d8/substrate/frame/psm/src/lib.rs#L1760) reads `total_issuance >= total_psm_debt`.
[Check 8](https://github.com/paritytech/polkadot-sdk/blob/d2e08992098ac61e13117d61420903e1e076d8d8/substrate/frame/psm/src/lib.rs#L1793) reads `reserve >= debt`, after converting `debt` to
external units. [Check 10](https://github.com/paritytech/polkadot-sdk/blob/d2e08992098ac61e13117d61420903e1e076d8d8/substrate/frame/psm/src/lib.rs#L1807) and [check 17](https://github.com/paritytech/polkadot-sdk/blob/d2e08992098ac61e13117d61420903e1e076d8d8/substrate/frame/psm/src/lib.rs#L1906) keep `PsmDebt`
within its per-asset and per-instance ceilings. All four bound `PsmDebt` from
above, so a pallet that records less debt than it owes passes every one of
them, further from the limit than before. Nothing bounds `PsmDebt` from below,
because the pallet stores balances, not flows.

The models add two counters per `(internal_asset, external_asset)` pair:
inflow on mint, outflow on redeem, both only ever increasing. The check is

```
PsmDebt[internal, external] == inflow[internal, external] - outflow[internal, external]
```

The two sides are equal, so a recorded debt that is too low fails the check as
readily as one that is too high. Measured against an injected understatement
of one unit per redeem: 200,000 fuzzed commands passed under the seventeen
checks, and this check failed at command 1.

Both counters hold internal units, the unit of `PsmDebt`. An earlier version
counted external amounts and compared their difference against `PsmDebt`. That
version fails on the first mint of any asset whose decimals differ from the
internal asset, since the two sides are then scaled by `10^|Δdecimals|`.
`psm_multidecimal.qnt` states both versions and is the regression guard. The
error was in the invariant, not in `pallet-psm`.

## Files

| File | Modules | Purpose |
| --- | --- | --- |
| `psm.qnt` | `psm`, `psm_correct`, `psm_buggy` | One asset, no fees, no decimals. `psm` takes `understateDebtOnMint`; `psm_correct` instantiates it false, `psm_buggy` true. Start here. |
| `psm_multidecimal.qnt` | `psm_multidecimal` | The two versions of the flow counters, external units and internal units, side by side. |
| `psm_extended.qnt` | `psm_extended` | Three decimal regimes, per-asset fees and ceilings, donations, asset lifecycle, governance levels, two users. |

`psm_multidecimal` and `psm_extended` are separate abstraction levels rather
than extensions of `psm`. Each declares its own state, so the three share no
declarations.

All three models cover a single PSM instance. `PsmDebt` is a double map in the
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

The negative controls must fail:

```
quint verify psm.qnt --main=psm_buggy --invariant=safetyInvariant --max-steps=10
quint run psm_multidecimal.qnt --invariant=proposalInvariant --max-steps=10
```

## Results

No invariant violation was found in the pallet as merged.

| Run | Result |
| --- | --- |
| `psm_correct`, Apalache, 10 steps | no violation, 32s |
| `psm_extended` `hardInvariant`, 3000 traces of 60 steps | no violation |
| `psm_multidecimal` `fixedInvariant`, 2000 traces of 60 steps | no violation |
| `psm_multidecimal` `proposalInvariant` | violation, 25ms |
| `psm_buggy`, Apalache, 10 steps | violation, 5s |
| `psm_buggy`, simulation | violation on the first mint |

The last three rows are negative controls: each states something known to be
false, and the toolchain reports it.
