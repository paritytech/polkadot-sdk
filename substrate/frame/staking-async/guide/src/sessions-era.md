# Sessions and Eras

Sessions and Eras are two key time-related concepts in Polkadot's staking system. They govern
validator rotation and staking reward distribution.

## Session

A session is the shortest unit of time in which the active validator set may change. On
Polkadot, a session lasts 4 hours and is determined by block timestamps, ensuring stable
timings even if block times vary slightly.

Session management is handled by `pallet-session` on the Relay Chain, which interacts with staking
logic via the `SessionManager` trait:

```rust,noplayground
{{#include ../../../session/src/lib.rs:trait_session_manager}}
```

The `pallet-staking-async-ah-client` (AHC) implements this trait with two main responsibilities:
- On every session change, it dispatches an XCM message to the Asset Hub, including a timestamp if
  a new validator set is activated.
- It receives new validator set from the Asset Hub and buffers them for later use.

### Validator Set Activation Flow

When `SessionManager::new_session()` is called:
- If a new validator set is available, it is returned to `pallet-session`.
- Otherwise `None` is returned and the current set remains active.

The validator set activation follows a pipelined model:
- Let validator set _X_ arrive in session _N_.
- It is buffered by AHC.
- At session _N + 1_, AHC hands _X_ to `pallet-session`.
- At session _N + 2_, _X_ is activated.
- The session change message at that point includes a timestamp used by the Asset Hub.

## Era

An era is a longer unit of time in which validators are elected and actively participate in
responsibilities such as block production, parachain backing, and dispute resolution. By
default, an era spans 6 sessions, though this is configurable.

### Era-triggered Validator Set Changes

In regular operation, validator elections are triggered toward the end of an era. Additionally,
if a session contains major offences that threaten economic security, `pallet-session` can
force an early election to replace the compromised validator set.

(TODO: not yet implemented.)

### Reward and Inflation Distribution

> TODO: Move this to `rewards.md`

Rewards and inflation are calculated per era:
- Validators earn era points by producing blocks, backing parachains, and resolving disputes.
- At era end, a target inflation value is calculated.
- This inflation is distributed proportionally based on each validator’s era points.
- Nominators receive rewards based on their backing.

## Era Rotation (Async Staking)

In `pallet-staking-async`, two key storage items manage era rotation:
- [CurrentEra](https://paritytech.github.io/polkadot-sdk/master/pallet_staking_async/type.CurrentEra.html)
- [ActiveEra](https://paritytech.github.io/polkadot-sdk/master/pallet_staking_async/storage_types/struct.ActiveEra.html)

When `CurrentEra == ActiveEra`, no new era is being planned.

### Triggering a New Election

Era planning begins when the active era’s age exceeds a threshold:

- On every session change, `pallet-staking-async` checks the era age by comparing the current
  session index with the start recorded in [`BondedEras`](https://paritytech.github.io/polkadot-sdk/master/pallet_staking_async/storage_types/struct.BondedEras.html).
- If `active_era_age >= SessionsPerEra - PlanningEraOffset`, a new election begins.

In Polkadot:
- `SessionsPerEra = 6`
- `PlanningEraOffset = 1`

Thus, elections are triggered at the start of session 5 of the current era. `CurrentEra` is
incremented at this point.

### Validator Set Election and Application

The validator set selected during election is identified by the value of `CurrentEra`.

- Once elections complete (expected within 1 session), the resulting validator set is sent to
  the Relay Chain.
- On the Relay Chain, this set is buffered by `pallet-staking-async-ah-client`.
- At the start of the next session, it is returned to `pallet-session`, which activates it in
  the session that follows.

This flow is designed to ideally maintain the configured `SessionsPerEra`, but it is not
strictly guaranteed. The `PlanningEraOffset` value should reflect the expected (rounded-up)
number of sessions needed to complete an election. If the election takes longer than expected,
the system handles it gracefully: the election simply concludes later, and the resulting era
may be longer than `SessionsPerEra`, depending on the delay.

### Era Duration Tracking

When the new validator set is applied, the session change message from the Relay Chain includes
the block timestamp of activation. `pallet-staking-async` uses this timestamp to calculate the
time duration of the era.

### Configuration
The era duration and election offset are configurable via the `Config` trait in
`pallet-staking-async` using the following parameters:

```rust,noplayground
{{#include ../../src/pallet/mod.rs:era_config}}
```
