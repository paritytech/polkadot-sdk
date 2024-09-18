# Distribution Pallet
## Overview

The **Distribution Pallet** handles the distribution of whitelisted projects rewards.

For now only one reward distribution pattern has been implemented, but the pallet could be extended
to offer to the user claiming rewards for a project, a choice between more than one distribution pattern.

The **Distribution Pallet** receives a list of Whitelisted/Nominated Projects with
their respective calculated rewards. For each project, it will create a corresponding
spend that will be stored until the project reward can be claimed.
At the moment, the reward claim period start corresponds to:
[beginning of an ***Epoch_Block*** + ***BufferPeriod***] (The ***BufferPeriod*** can be configured in the runtime).

### Terminology

- **PotId:** Pot containing the funds used to pay the rewards.
- **BufferPeriod:** minimum required buffer time period between project nomination and reward claim.

## Interface

### Dispatchable Functions

#### Public

These calls can be made from any externally held account capable of creating
a signed extrinsic.

Basic actions:
- `claim_reward_for` - From this extrinsic any user can claim a reward for a nominated/whitelisted project.

License: Apache-2.0
