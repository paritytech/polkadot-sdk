# Sessions and Eras (WIP)

TODO @Ank4n @kianenigma

Sessions and Eras are two key time-related concepts in Polkadot's staking system. They govern validator rotation and staking reward distribution.

## Session

A session is the shortest time period during which the active validator set may change. On Polkadot, a session lasts 4 hours and is driven by block timestamps, so it does not drift even if block production slows down.

## Era

An era is a longer period over which validators are elected and actively participate in duties such as block production, backing parachain blocks, and dispute resolution. An era typically spans 6 sessions, though this is configurable. In edge cases, it may last longer if a new validator set isn't elected in time.

### Slash-triggered Validator Replacement

If a session includes major offences or slashes that significantly impact economic security, `pallet-session` can notify `pallet-staking` to begin a new validator election at the next session boundary to replace the compromised set.

### Rewards and Inflation

Eras are the units over which inflation and staking rewards are calculated:
- Validators accumulate era points for actions such as block authorship, backing parablocks, and participating in disputes.
- At the end of an era, a target inflation amount (i.e., tokens to mint) is determined.
- This inflation is distributed proportionally based on the era points earned by each validator.
- Rewards are then distributed to the nominators who backed those validators.

### Session and Era Coordination (Async Staking)

In the async staking model, session and era coordination is distributed between the Relay Chain (RC) and the Asset Hub (AH).

#### Session Change Flow

- At the end of each session, the Relay Chain dispatches a session change message to the Asset Hub via XCM. This message includes the session index and the block timestamp at which the new validator set was applied.
- When the Asset Hub receives this message, it marks the session as the start of a new era. This timestamp is later used for inflation and reward calculations.

#### Era Change and Elections

- On the 5th session change of an era, `AH::pallet-staking-async` creates a paged snapshot of validator votes and initiates a new validator election.
- Elections are optimized to complete well before the session ends—ideally with at least 25% of the session remaining to allow time for processing.
- Once the election results are ready, the new validator set is sent back to the Relay Chain.

#### Validator Set Activation

- `RC::pallet-staking-async-ah-client` buffers the upcoming validator set and hands it off to `pallet-session` at the start of the next session.
- After the new validator set is activated, `RC::pallet-staking-async-ah-client` dispatches a new `SessionChange` XCM message to the Asset Hub with the current block timestamp, continuing the coordination loop.

// TODO: @ank4n Link all code paths.
// Review/explain session_rotation.rs.
