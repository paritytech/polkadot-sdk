---
description: Meeting notes for our workshop at the Lisbon Parachain Summit
---

# Bridges Workshop

**Date:** 1 Dec 2022

**Attendees:**

* Vincent, Alistair (Snowfork)
* Ricardo Ruis, Robert Habermeier, Robert Hambrock, Mattias Petter Johansson (Parity)
* Syed Hosseini (W3F)
* Sourabh Niyogi

There were a few other attendees, I just didn't manage to get their names :sweat\_smile:

## Cross-chain Governance

The discussion in the workshop seems to validate our [plan for cross-chain governance](../architecture/governance.md). Specifically, for _fallback_ governance on Ethereum, a voting collective will be empowered to upgrade only the BEEFY light client contract.

### Next Steps

Initiate a discussion on Polkadot forum to

* Socialize this idea further in the community
* Determine the membership of the collective
* Discuss regulatory exposure

Owner: Vincent

## Message Batching

For the polkadot→ethereum path, move [message batching](https://docs.snowbridge.network/architecture/channels#\_faw9foweutag) to application layer. This simplifies our channel protocol, message dispatch logic, and fee calculations.

For example, batched XCM instructions could be handled by the XCM executor contract on Ethereum.

Owner: Vincent

## Ethereum PoS Light Client

Most discussions on this topic were related to BLS.

In our light client, aggregating public keys and verifying a single BLS signature takes roughly 1/4 of the block weight, which isn’t sustainable in the long-term, especially on BridgeHub.

Mitigations:

1. Asynchronous backing may improve this by increasing blockspace.
2. A huge performance booster will be host functions for BLS-12-381 signature verification.
3. Should also investigate using a ZK-SNARKS circuit for signature verification in Substrate.
   1. Apparently W3F already has a working prototype for this, which we can potentially adapt.

### Next Steps

1. Parity to figure out the situation with host functions. Looks like there needs to be some kind of RFC process for the community to propose new host functions
2. In the longer-term, Snowfork should look at ZK-SNARKS for further improving efficiency of signature verification on Substrate.
3. Snowfork (Clara) to design and implement safeguards against [long-range attacks](https://near.org/blog/long-range-attacks-and-a-new-fork-choice-rule/).

## Defense in Depth

Snowfork’s proposed circuit breaker on collateral withdrawals won’t actually increase security much, since you can’t really have a circuit breaker on cross-chain governance. And if cross-chain governance is exploited, then everything controlled by governance is exploitable too.

Simple limits on TVL may still work with XCMv3 model, will need to check (Owner: Vincent)

Our defense in depth strategy therefore needs to focus on implementation quality:

1. Unit and Integration Testing. Especially tests of an adversarial nature.
2. Fuzz Testing
3. Bug Bounties on Rococo and Kusama
4. Multiple redundant security audits

Owner: Snowfork
