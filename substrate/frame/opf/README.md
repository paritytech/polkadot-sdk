# OPF Pallet
## Overview

The **OPF Pallet** handles the Optimistic Project Funding.
It allows users to nominate projects (whitelisted in OpenGov) with their DOT.
This mechanism will be funded with a constant stream of DOT taken directly from inflation
and distributed to projects based on the proportion of DOT that has nominated them.
The project rewards distribution is handled by the **Distribution Pallet**
The voting round timeline is described below for someone voting for a project with no conviction round_0 and
for another project with a conviction of 1x in round_1:

```
|----------Voting_Round_0-----------|----------Voting_Round_1-----------|
|----user_votes----|--funds0_locked-|----user_votes----|--funds1_locked-|--funds1_locked-|
|------------------|--Distribution--|------------------|--Distribution--|

```


**Relevant Links:**
- *Full description of the mechanism that was approved*: https://docs.google.com/document/d/1cl6CpWyqX7NCshV0aYT5a8ZTm75PWcLrEBcfk2I1tAA/edit#heading=h.hh40wjcakxp9

- *Polkadot's economics Forum post*: https://forum.polkadot.network/t/polkadots-economics-tools-to-shape-the-forseeable-future/8708?u=lolmcshizz

- *Project discussion TG*: https://t.me/parachainstaking

### Terminology

- **MaxWhitelistedProjects:** Maximum number of Whitelisted projects that can be handled by the pallet.
- **VoteLockingPeriod:** Period during which voting is disabled.
- **VotingPeriod:** Period during which voting is enabled.
- **TemporaryRewards:** For test purposes only ⇒ used as a substitute for the inflation portion used for the rewards.

## Interface

### Dispatchable Functions

#### Public

These calls can be made from any externally held account capable of creating
a signed extrinsic.

**Basic actions:**
- `vote` - This extrinsic allows users to [vote for/nominate] a whitelisted project using their funds.
- `remove_vote` - This extrinsic allows users to remove a cast vote, as long as it is within the vote-casting period.
    The user can add a conviction to the amount appointed to the vote.
    With a conviction of x2 for example, one additional funds locking period will be added after the end of the round,
    as shown in the diagram above.
- `unlock_funds` - This extrinsic allows the user to unlock his funds, provided that the funds locking period has ended.

License: Apache-2.0
