# Polkadot <> Kusama Bridge Overview

This document describes how we use all components, described in the [High-Level Bridge
Documentation](./high-level-overview.md), to build the XCM bridge between Kusama and Polkadot. In this case, our
components merely work as a XCM transport (like XCMP/UMP/HRMP), between chains that are not a part of the same consensus
system.

The overall architecture may be seen in [this diagram](./polkadot-kusama-bridge.html).

## Bridge Hubs

All operations at relay chain are expensive. Ideally all non-mandatory transactions must happen on parachains. That's
why we are planning to have two parachains - Polkadot Bridge Hub under Polkadot consensus and Kusama Bridge Hub under
Kusama consensus.

The Bridge Hub will have all required bridge pallets in its runtime. We hope that later, other teams will be able to use
our bridge hubs too and have their pallets there.

The Bridge Hub will use the base token of the ecosystem - KSM at Kusama Bridge Hub and DOT at Polkadot Bridge Hub. The
runtime will have minimal set of non-bridge pallets, so there's not much you can do directly on bridge hubs.

## Connecting Parachains

You won't be able to directly use bridge hub transactions to send XCM messages over the bridge. Instead, you'll need to
use other parachains transactions, which will use HRMP to deliver messages to the Bridge Hub. The Bridge Hub will just
queue these messages in its outbound lane, which is dedicated to deliver messages between two parachains.

Our first planned bridge will connect the Polkadot and Kusama Asset Hubs. A bridge between those two parachains would
allow Asset Hub Polkadot accounts to hold wrapped KSM tokens and Asset Hub Kusama accounts to hold wrapped DOT tokens.

For that bridge (pair of parachains under different consensus systems) we'll be using the lane 00000000. Later, when
other parachains will join the bridge, they will be using other lanes for their messages.

## Running Relayers

We are planning to run our own complex relayer for the lane 00000000. The relayer will relay Kusama/Polkadot GRANDPA
justifications to the bridge hubs at the other side. It'll also relay finalized Kusama Bridge Hub and Polkadot Bridge
Hub heads. This will only happen when messages will be queued at hubs. So most of time relayer will be idle.

There's no any active relayer sets, or something like that. Anyone may start its own relayer and relay queued messages.
We are not against that and, as always, appreciate any community efforts. Of course, running relayer has the cost. Apart
from paying for the CPU and network, the relayer pays for transactions at both sides of the bridge. We have a mechanism
for rewarding relayers.

### Compensating the Cost of Message Delivery Transactions

One part of our rewarding scheme is that the cost of message delivery, for honest relayer, is zero. The honest relayer
is the relayer, which is following our rules:

- we do not reward relayers for submitting GRANDPA finality transactions. The only exception is submitting mandatory
  headers (headers which are changing the GRANDPA authorities set) - the cost of such transaction is zero. The relayer
  will pay the full cost for submitting all other headers;

- we do not reward relayers for submitting parachain finality transactions. The relayer will pay the full cost for
  submitting parachain finality transactions;

- we compensate the cost of message delivery transactions that have actually delivered the messages. So if your
  transaction has claimed to deliver messages `[42, 43, 44]`, but, because of some reasons, has actually delivered
  messages `[42, 43]`, the transaction will be free for relayer. If it has not delivered any messages, then the relayer
  pays the full cost of the transaction;

- we compensate the cost of message delivery and all required finality calls, if they are part of the same
  [`frame_utility::batch_all`](https://github.com/paritytech/substrate/blob/891d6a5c870ab88521183facafc811a203bb6541/frame/utility/src/lib.rs#L326)
  transaction. Of course, the calls inside the batch must be linked - e.g. the submitted parachain head must be used to
  prove messages. Relay header must be used to prove parachain head finality. If one of calls fails, or if they are not
  linked together, the relayer pays the full transaction cost.

Please keep in mind that the fee of "zero-cost" transactions is still withdrawn from the relayer account. But the
compensation is registered in the `pallet_bridge_relayers::RelayerRewards` map at the target bridge hub. The relayer may
later claim all its rewards later, using the `pallet_bridge_relayers::claim_rewards` call.

*A side note*: why we don't simply set the cost of useful transactions to zero? That's because the bridge has its cost.
If we won't take any fees, it would mean that the sender is not obliged to pay for its messages. And Bridge Hub
collators (and, maybe, "treasury") are not receiving any payment for including transactions. More about this later, in
the [Who is Rewarding Relayers](#who-is-rewarding-relayers) section.

### Message Delivery Confirmation Rewards

In addition to the "zero-cost" message delivery transactions, the relayer is also rewarded for:

- delivering every message. The reward is registered during delivery confirmation transaction at the Source Bridge Hub.;

- submitting delivery confirmation transaction. The relayer may submit delivery confirmation that e.g. confirms delivery
  of four messages, of which the only one (or zero) messages is actually delivered by this relayer. It receives some fee
  for confirming messages, delivered by other relayers.

Both rewards may be claimed using the `pallet_bridge_relayers::claim_rewards` call at the Source Bridge Hub.

### Who is Rewarding Relayers

Obviously, there should be someone who is paying relayer rewards. We want bridge transactions to have a cost, so we
can't use fees for rewards. Instead, the parachains using the bridge, use sovereign accounts on both sides of the bridge
to cover relayer rewards.

Bridged Parachains will have sovereign accounts at bridge hubs. For example, the Kusama Asset Hub will have an account
at the Polkadot Bridge Hub. The Polkadot Asset Hub will have an account at the Kusama Bridge Hub. The sovereign accounts
are used as a source of funds when the relayer is calling the `pallet_bridge_relayers::claim_rewards`.

Since messages lane is only used by the pair of parachains, there's no collision between different bridges. E.g. Kusama
Asset Hub will only reward relayers that are delivering messages from Kusama Asset Hub. The Kusama Asset Hub sovereign
account is not used to cover rewards of bridging with some other Polkadot Parachain.

### Multiple Relayers and Rewards

Our goal is to incentivize running honest relayers. But we have no relayers sets, so at any time anyone may submit
message delivery transaction, hoping that the cost of this transaction will be compensated. So what if some message is
currently queued and two relayers are submitting two identical message delivery transactions at once? Without any
special means, the cost of first included transaction will be compensated and the cost of the other one won't. A honest,
but unlucky relayer will lose some money. In addition, we'll waste some portion of block size and weight, which may be
used by other useful transactions.

To solve the problem, we have two signed extensions ([generate_bridge_reject_obsolete_headers_and_messages!
{}](../bin/runtime-common/src/lib.rs) and
[RefundRelayerForMessagesFromParachain](../bin/runtime-common/src/refund_relayer_extension.rs)), that are preventing
bridge transactions with obsolete data from including into the block. We are rejecting following transactions:

- transactions, that are submitting the GRANDPA justification for the best finalized header, or one of its ancestors;

- transactions, that are submitting the proof of the current best parachain head, or one of its ancestors;

- transactions, that are delivering already delivered messages. If at least one of messages is not yet delivered, the
  transaction is not rejected;

- transactions, that are confirming delivery of already confirmed messages. If at least one of confirmations is new, the
  transaction is not rejected;

- [`frame_utility::batch_all`](https://github.com/paritytech/substrate/blob/891d6a5c870ab88521183facafc811a203bb6541/frame/utility/src/lib.rs#L326)
  transactions, that have both finality and message delivery calls. All restrictions from the [Compensating the Cost of
  Message Delivery Transactions](#compensating-the-cost-of-message-delivery-transactions) are applied.
