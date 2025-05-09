# Slashing

This section outlines how slashing is handled in the async staking system across the Relay Chain (RC) and Asset Hub (AH), with a focus on offence reporting, queuing, and eventual slash application logic.

## Overview

Slashing is the mechanism by which validators and their nominators are penalized for misbehavior (e.g., equivocation). It is designed to be fair, deferred, and page-based in execution, and coordinated across RC and AH.

## Key stages

There are three main stages in the slashing process:
- Offence reporting
- Slash computation
- Slash Application

We will go through each of these stages in detail.

### 1. Offence Reporting

#### Relay Chain
Offences are detected and reported through the following pallets on the Polkadot Relay Chain:

- `pallet-babe`
- `pallet-beefy`
- `pallet-grandpa`
- `parachains-slashing`

These offences are propagated to `pallet-offences`, where they are verified for validity and session linkage. Once verified, the offence is passed to `pallet-staking-async-ah-client`, which implements the `OnOffenceHandler` trait.

If the offence belongs to a validator who has already another offence reported for the era, we only keep the offence if the new one has a higher `slash_perbill` than the previous one. And we make a note of the new highest `slash_perbill` for the era in the storage map `ValidatorSlashInEra`. If the offence `perbill` is lower, we simply ignore it.

 ```rust,noplayground
{{#include ../../../../primitives/staking/src/offence.rs:trait_on_offence_handler}}
```

**Validator Disabling**

The offending validator is **disabled** via `pallet-session` if the offence occurred in the current active era.

**Offence Dispatching**

The offence, along with the computed slash fraction, is **dispatched** as an XCM message to the Asset Hub (`pallet-staking-async-rc-client`) for further handling.

#### Asset Hub

**Offence Queue**  
Offences are enqueued for processing in subsequent blocks with a pointer to the last page of the validator exposure for the offence era. This pointer is later used when computing slash, as we want to process one page (i.e. a bounded set of nominators) per block.

It uses the following storages to queue the offence:
- [OffenceQueue](https://paritytech.github.io/polkadot-sdk/master/pallet_staking_async/type.OffenceQueue.html)
- [OffenceQueueEras](https://paritytech.github.io/polkadot-sdk/master/pallet_staking_async/type.OffenceQueueEras.html)

### 2. Slash Computation (Asset Hub)
**Paged Application**

- A single validator’s slashes may span multiple pages, each covering a subset of nominators. In Polkadot, we can have upto 64 nominators per exposure page of a validator.
- Slash is computed based on one exposure page per block. For each offence in the queue, we compute the slash for the last page of the validator exposure and then decrement the page pointer.
- Once the page pointer is 0, the offence is removed from the queue.

**Priority of Offences**

- If there are offences from multiple eras, offences from **older eras** are prioritized over more recent ones.
- Within the same era, there is no priority between offences.

**Slash Computation Logic**

- Each offence is evaluated against the highest slash applied to the validator within the same era.
- If there was a previous offence for the validator in the same era, only the difference between the new and the previous slash is applied.
- We only apply the slash difference for nominator as well if they have been slashed for their exposure belonging to either the same validator or a different one for the same era.

**Storage**

Computed slashes are stored in [UnappliedSlashes](https://paritytech.github.io/polkadot-sdk/master/pallet_staking_async/type.UnappliedSlashes.html).

### 3. Slash Application (Asset Hub)

**Slash Schedule**

// TODO @ank4n (recheck the exact values for SlashDeferDuration and BondingDuration)
- Slashes are deferred by `SlashDeferDuration` eras.
- For example, slashes from offence era _X_ begin applying in era _X + SlashDeferDuration_.

**Application Loop**  
- One page unapplied slash is attempted per block over a full era as a system function (`fn on_initialize()`).
- Since the slashing begins at Era _X + 27_, and we want to ideally process all slashes by the start of Era _X + 28_, we have one full era of blocks to process the slashes.
- If not all slashes are applied within the era (unlikely but possible), the system will no longer attempt to apply them automatically. However, they can still be applied manually via the fee-less [Staking::apply_slash](https://paritytech.github.io/polkadot-sdk/master/pallet_staking_async/dispatchables/fn.apply_slash.html) extrinsic.

## Future Improvements

- Improve efficiency by dynamically adjusting how many pages are processed per block, based on weight.
- Add restrictions to prevent unbonding if a nominator has a pending slash.
- System task for applying slash for idle block space. Offchain monitoring of unapplied slashes.

