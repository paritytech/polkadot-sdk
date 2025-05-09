# Slashing

This section outlines how slashing is handled in the async staking system across the Relay Chain (RC) and Asset Hub (AH), with a focus on offence reporting, queuing, and eventual slash application logic.

## Overview

Slashing is the mechanism by which validators and their nominators are penalized for misbehavior (e.g., equivocation). It is designed to be fair, deferred, and page-based in execution, and coordinated across RC and AH.

## Offence Reporting and Routing

Offences are detected and reported through various pallets on the Relay Chain:

- `pallet-babe`
- `pallet-beefy`
- `pallet-grandpa`
- `parachains-slashing`

These offences are propagated to `pallet-offences`, where they are verified for validity and session linkage. Once verified, the offence is passed to `pallet-staking-async-ah-client`, which implements the `OnOffenceHandler` trait.

### RC-side Actions

- The offending validator is **disabled** via `pallet-session` if the offence occurred in the current active era.
- The offence, along with the computed slash fraction, is **dispatched** as an XCM message to the Asset Hub (`pallet-staking-async-rc-client`) for further handling.

## Offence Queue and Slash Computation (Asset Hub)

**Offence Queue**  
- Offences are enqueued for processing in subsequent blocks.  
- Only one validator page (i.e. a bounded set of nominators) is processed per block.

**Slash Computation**  
- Each offence is evaluated against the highest slash applied to the validator within the same era.
- If the offence is a **repeat** and has a **lower or equal** `slash_perbill`, it is ignored.
- If it has a **higher** `slash_perbill`, it is processed and replaces the previous one.

**Priority Rule**  
- Offences from **older eras** are always prioritized over more recent ones.

## Slash Application

**Slash Schedule**  
- Slashes are deferred by `SlashDeferDuration` eras.
- For example, slashes from offence era _X_ begin applying in era _X + 27_.

**Paged Application**  
- A single validator’s slashes may span multiple pages, each covering a subset of nominators.
- There are maximum of 64 nominators per Page.

**Application Loop**  
- One page is attempted per block over a full era as a system function.
  If not all slashes are applied within the era (which is unlikely), the system will no longer attempt to apply them automatically. However, they can still be applied manually via the fee-less `Staking::apply_slash` extrinsic.

## Future Improvements

- Improve efficiency by dynamically adjusting how many pages are processed per block, based on weight.
- Add restrictions to prevent unbonding if a nominator has a pending slash.

// TODO @ank4n: link all code paths. 
