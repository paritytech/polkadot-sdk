# Sessions and Eras (WIP)

TODO @Ank4n @kianenigma

Sessions and Eras are two key time-related concepts in Polkadot's staking system. They manage validator rotation and staking reward distribution.

## Session

A Session is the shortest time period during which the active validator set may change. On Polkadot, a session lasts 4 hours, and its timing is derived from the block timestamp—so it doesn’t drift even if block production slows down.

## Era

An Era is a longer period over which validators are elected and actively participate in duties like block production, backing parachain blocks, and dispute resolution. An era is typically 6 sessions long, but this is configurable. In edge cases, it may last longer if a new validator set isn't elected in time.

### Slash-triggered Validator Replacement

If a session sees major offences or slashes that materially impact economic security, pallet-session can notify pallet-staking to start a new validator election at the next session boundary to replace the compromised set.

### Rewards and Inflation

Eras are the units over which inflation and staking rewards are calculated:
- Validators accumulate era points for the work they do – such as block authorship, backing parablocks, and voting in disputes.
- At the end of the era, a target inflation amount (how many tokens to mint) is determined.
- The inflation is then distributed proportionally based on the points earned by each validator during the era.
- This reward is in-turn distributed to the nominators backing those validators.

// TODO: Add code paths and an example showing the lifecycle of a session and era change (with timeline, events, and outcomes).
