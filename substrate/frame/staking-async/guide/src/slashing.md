# Slashing

This section outlines how slashing is handled in the async staking system across the Relay Chain (RC) and Asset Hub (AH), focusing on offence reporting, queuing, and the eventual slash application logic.

## Overview

Slashing is the mechanism by which validators and their nominators are penalized for misbehavior (e.g., equivocation). It is designed to be fair, deferred, page-based in execution, and coordinated across RC and AH.

## Key Stages

There are three main stages in the slashing process:
- Offence Reporting
- Slash Computation
- Slash Application

We will go through each of these stages in detail.

### 1. Offence Reporting

#### Relay Chain

Offences are detected and reported through the following pallets on the Polkadot Relay Chain:

- `pallet-babe`
- `pallet-beefy`
- `pallet-grandpa`
- `parachains-slashing`

These offences are propagated to `pallet-offences`, where they are verified for session validity. Once verified, the offence is passed to `pallet-staking-async-ah-client`, which implements the `OnOffenceHandler` trait.

If the validator already has an offence recorded in the current era, we retain only the offence with the higher `slash_perbill`. The highest perbill for the era is stored in `ValidatorSlashInEra`. If the new offence has a lower perbill, it is ignored.

```rust,noplayground
{{#include ../../../../primitives/staking/src/offence.rs:trait_on_offence_handler}}
```

**Validator Disabling**</br>
The offending validator is **disabled** via `pallet-session` if the offence occurred in the current active era.

**Offence Dispatching**</br>
The offence, along with the computed slash fraction, is **dispatched** as an XCM message to the Asset Hub (`pallet-staking-async-rc-client`) for further handling.

#### Asset Hub

**Offence Queue**</br>
Offences are enqueued for processing in subsequent blocks, each carrying a pointer to the last page of the validator’s exposure for the offence era. This pointer is used to track paged slash computation, with one page (i.e., a bounded set of nominators) processed per block.

The following storages are used to queue offences:
- [`OffenceQueue`](https://paritytech.github.io/polkadot-sdk/master/pallet_staking_async/type.OffenceQueue.html)
- [`OffenceQueueEras`](https://paritytech.github.io/polkadot-sdk/master/pallet_staking_async/type.OffenceQueueEras.html)

### 2. Slash Computation (Asset Hub)

**Paged Application**  
- A validator’s slashes may span multiple pages, each containing a subset of nominators.
- In Polkadot, each exposure page contains up to 64 nominators.
- Slash computation is performed one page per block. For each offence in the queue, the slash is computed for the current page, and the page pointer is decremented.
- When the pointer reaches 0, the offence is removed from the queue.

**Offence Prioritization**  
- Offences from **older eras** are always prioritized over newer ones.
- Within the same era, there is no specific order of processing. But all the pages of an offence is processed in order before moving to the next offence.

**Slash Computation Logic**  
- Each offence is compared against the highest slash already recorded for the validator in the same era.
- If it’s a repeat offence, only the **difference** in slash is applied.
- The same logic applies to nominators: if they were already slashed for exposure to the same validator (or another validator) in the same era, only the remaining slash is applied.

**Storage**  
Computed slashes are stored in:
- [`UnappliedSlashes`](https://paritytech.github.io/polkadot-sdk/master/pallet_staking_async/type.UnappliedSlashes.html)

### 3. Slash Application (Asset Hub)

**Slash Schedule**</br>
// TODO @ank4n (recheck the exact values for SlashDeferDuration and BondingDuration)
- Slashes are deferred by `SlashDeferDuration` eras.
- For example, slashes from era _X_ begin applying in era _X + SlashDeferDuration_.

**Application Loop**  
- One page of unapplied slashes is processed per block over a full era using `on_initialize()`.
- For example, if slashing starts at era _X + 27_, we aim to apply all slashes by the start of era _X + 28_.
- If some slashes remain unapplied after the era ends (unlikely but possible), the system stops applying them automatically. They can still be applied manually via the feeless [`staking::apply_slash`](https://paritytech.github.io/polkadot-sdk/master/pallet_staking_async/dispatchables/fn.apply_slash.html) extrinsic.

## Future Improvements

- Improve efficiency by dynamically adjusting the number of pages processed per block based on block weight.
- Add restrictions to prevent unbonding if a nominator has a pending slash.
- Introduce a system task to opportunistically apply slashes in idle block space.
- Implement offchain monitoring for unapplied slashes.
