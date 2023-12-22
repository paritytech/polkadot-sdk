# Relayers

{% hint style="info" %}
Relayers are **permissionless and trustless**. This means that anyone can operate a relayer for channels they are interested in.
{% endhint %}

A relay is a piece of software running offchain that watches two blockchains and relays messages across them. The implementation of the relayer in our bridge is not part of the core protocol, as it is offchain and so is untrusted. Of course, some relayer still needs to be running in order for the bridge to function, but it only needs to conform to the protocol defined by on-chain requirements.

We provide relayer software that will be run by incentivized relayers to keep the bridge active, but the design and implementation of the relayer are not relevant for understanding the trustless bridge protocol.

## Polkadot->Ethereum

### BEEFY relay

Relays signed BEEFY commitments and proofs from a Polkadot relay chain to the BEEFY light client contract on Ethereum.

### Message relay

Relays message commitments and proofs from BridgeHub to inbound channel contracts on Ethereum

## Ethereum->Polkadot

### Header relay

Relays the following objects to the Ethereum light client pallet on the BridgeHub parachain:

* Beacon chain headers
* Execution chain headers
* Sync Committees

### Message relay

Relays messages emitted by outbound channel contracts on Ethereum.
