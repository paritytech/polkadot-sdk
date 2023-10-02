# Staking Parachain

*Note: the Staking Parachain implementation is still in experimental phase. Both design and
implementation is fluid and may change at any time. See <https://hackmd.io/okXqe3csRd2Qmo6eXCNQ3Q>
for more information on the design decisions.*

Implementation of Staking Chain, a blockchain to support staking in the Polkadot and Kusama
networks.

**Staking Chain allows users to**:

- Teleport DOT from Asset Hub to use as staking funds (native staking).
- Use reserved DOT asset in Asset Hub as staking funds (remote  staking).
- Set the intent to become a validator.
- Set the intent to become a nominator and/or join nomination pools.
- Stake assets natively and remotely through the Assets Hub.

### Staking Chain and Assets Hub

The Staking Chain provides the following information to the Assets Hub:

- When to mint a new batch of inflation DOT tokens.
- The previous era durantion and the overall amount of staked DOT.

### Staking Chain and Relay Chain:

The Staking Chain provides the following information to the Relay Chain:

- When a new validator set should be enabled.
- Which validators should be part of the new validator set.
- Which active validators should be disabled (due to offence).

The Staking Chain receives the following information from the Relay Chain:

- Reward points for block authors.
- Validator offences.

Staking Chain must stay fully aligned with the Realy Chain it is connected to. As such, it will
accept the Relay Chain's governance as its own. In addition, we assume that the Relay Chain trusts
the Staking Chain, namely when the Stackin Chain:

- Announces a new set active validators.
- Processes offences and applies slashes.
- Accounces the staked amount, era duration and transition when calculating the inflation minting.

### Reserving Assets on Staking Chain

System parachains (e.g. governance) may reserve native DOT assets from the Staking Chain to use
locally.
