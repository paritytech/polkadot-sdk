# Bridge Messages Pallet

The messages pallet is used to deliver messages from source chain to target chain. Message is (almost) opaque to the
module and the final goal is to hand message to the message dispatch mechanism.

## Contents

- [Overview](#overview)
- [Message Workflow](#message-workflow)
- [Integrating Message Lane Module into Runtime](#integrating-messages-module-into-runtime)
- [Non-Essential Functionality](#non-essential-functionality)
- [Weights of Module Extrinsics](#weights-of-module-extrinsics)

## Overview

Message lane is an unidirectional channel, where messages are sent from source chain to the target chain. At the same
time, a single instance of messages module supports both outbound lanes and inbound lanes. So the chain where the module
is deployed (this chain), may act as a source chain for outbound messages (heading to a bridged chain) and as a target
chain for inbound messages (coming from a bridged chain).

Messages module supports multiple message lanes. Every message lane is identified with a 4-byte identifier. Messages
sent through the lane are assigned unique (for this lane) increasing integer value that is known as nonce ("number that
can only be used once"). Messages that are sent over the same lane are guaranteed to be delivered to the target chain in
the same order they're sent from the source chain. In other words, message with nonce `N` will be delivered right before
delivering a message with nonce `N+1`.

Single message lane may be seen as a transport channel for single application (onchain, offchain or mixed). At the same
time the module itself never dictates any lane or message rules. In the end, it is the runtime developer who defines
what message lane and message mean for this runtime.

In our [Kusama<>Polkadot bridge](../../docs/polkadot-kusama-bridge-overview.md) we are using lane as a channel of
communication between two parachains of different relay chains. For example, lane `[0, 0, 0, 0]` is used for Polkadot <>
Kusama Asset Hub communications. Other lanes may be used to bridge other parachains.

## Message Workflow

The pallet is not intended to be used by end users and provides no public calls to send the message. Instead, it
provides runtime-internal method that allows other pallets (or other runtime code) to queue outbound messages.

The message "appears" when some runtime code calls the `send_message()` method of the pallet. The submitter specifies
the lane that they're willing to use and the message itself. If some fee must be paid for sending the message, it must
be paid outside of the pallet. If a message passes all checks (that include, for example, message size check, disabled
lane check, ...), the nonce is assigned and the message is stored in the module storage. The message is in an
"undelivered" state now.

We assume that there are external, offchain actors, called relayers, that are submitting module related transactions to
both target and source chains. The pallet itself has no assumptions about relayers incentivization scheme, but it has
some callbacks for paying rewards. See [Integrating Messages Module into
runtime](#Integrating-Messages-Module-into-runtime) for details.

Eventually, some relayer would notice this message in the "undelivered" state and it would decide to deliver this
message. Relayer then crafts `receive_messages_proof()` transaction (aka delivery transaction) for the messages module
instance, deployed at the target chain. Relayer provides its account id at the source chain, the proof of message (or
several messages), the number of messages in the transaction and their cumulative dispatch weight. Once a transaction is
mined, the message is considered "delivered".

Once a message is delivered, the relayer may want to confirm delivery back to the source chain. There are two reasons
why it would want to do that. The first is that we intentionally limit number of "delivered", but not yet "confirmed"
messages at inbound lanes (see [What about other Constants in the Messages Module Configuration
Trait](#What-about-other-Constants-in-the-Messages-Module-Configuration-Trait) for explanation). So at some point, the
target chain may stop accepting new messages until relayers confirm some of these. The second is that if the relayer
wants to be rewarded for delivery, it must prove the fact that it has actually delivered the message. And this proof may
only be generated after the delivery transaction is mined. So relayer crafts the `receive_messages_delivery_proof()`
transaction (aka confirmation transaction) for the messages module instance, deployed at the source chain. Once this
transaction is mined, the message is considered "confirmed".

The "confirmed" state is the final state of the message. But there's one last thing related to the message - the fact
that it is now "confirmed" and reward has been paid to the relayer (or at least callback for this has been called), must
be confirmed to the target chain. Otherwise, we may reach the limit of "unconfirmed" messages at the target chain and it
will stop accepting new messages. So relayer sometimes includes a nonce of the latest "confirmed" message in the next
`receive_messages_proof()` transaction, proving that some messages have been confirmed.

## Integrating Messages Module into Runtime

As it has been said above, the messages module supports both outbound and inbound message lanes. So if we will integrate
a module in some runtime, it may act as the source chain runtime for outbound messages and as the target chain runtime
for inbound messages. In this section, we'll sometimes refer to the chain we're currently integrating with, as "this
chain" and the other chain as "bridged chain".

Messages module doesn't simply accept transactions that are claiming that the bridged chain has some updated data for
us. Instead of this, the module assumes that the bridged chain is able to prove that updated data in some way. The proof
is abstracted from the module and may be of any kind. In our Substrate-to-Substrate bridge we're using runtime storage
proofs. Other bridges may use transaction proofs, Substrate header digests or anything else that may be proved.

**IMPORTANT NOTE**: everything below in this chapter describes details of the messages module configuration. But if
you're interested in well-probed and relatively easy integration of two Substrate-based chains, you may want to look at
the [bridge-runtime-common](../../bin/runtime-common/) crate. This crate is providing a lot of helpers for integration,
which may be directly used from within your runtime. Then if you'll decide to change something in this scheme, get back
here for detailed information.

### General Information

The messages module supports instances. Every module instance is supposed to bridge this chain and some bridged chain.
To bridge with another chain, using another instance is suggested (this isn't forced anywhere in the code, though). Keep
in mind, that the pallet may be used to build virtual channels between multiple chains, as we do in our [Polkadot <>
Kusama bridge](../../docs/polkadot-kusama-bridge-overview.md). There, the pallet actually bridges only two parachains -
Kusama Bridge Hub and Polkadot Bridge Hub. However, other Kusama and Polkadot parachains are able to send (XCM) messages
to their Bridge Hubs. The messages will be delivered to the other side of the bridge and routed to the proper
destination parachain within the bridged chain consensus.

Message submitters may track message progress by inspecting module events. When Message is accepted, the
`MessageAccepted` event is emitted. The event contains both message lane identifier and nonce that has been assigned to
the message. When a message is delivered to the target chain, the `MessagesDelivered` event is emitted from the
`receive_messages_delivery_proof()` transaction. The `MessagesDelivered` contains the message lane identifier and
inclusive range of delivered message nonces.

The pallet provides no means to get the result of message dispatch at the target chain. If that is required, it must be
done outside of the pallet. For example, XCM messages, when dispatched, have special instructions to send some data back
to the sender. Other dispatchers may use similar mechanism for that.
### How to plug-in Messages Module to Send Messages to the Bridged Chain?

The `pallet_bridge_messages::Config` trait has 3 main associated types that are used to work with outbound messages. The
`pallet_bridge_messages::Config::TargetHeaderChain` defines how we see the bridged chain as the target for our outbound
messages. It must be able to check that the bridged chain may accept our message - like that the message has size below
maximal possible transaction size of the chain and so on. And when the relayer sends us a confirmation transaction, this
implementation must be able to parse and verify the proof of messages delivery. Normally, you would reuse the same
(configurable) type on all chains that are sending messages to the same bridged chain.

The last type is the `pallet_bridge_messages::Config::DeliveryConfirmationPayments`. When confirmation
transaction is received, we call the `pay_reward()` method, passing the range of delivered messages.
You may use the [`pallet-bridge-relayers`](../relayers/) pallet and its
[`DeliveryConfirmationPaymentsAdapter`](../relayers/src/payment_adapter.rs) adapter as a possible
implementation. It allows you to pay fixed reward for relaying the message and some of its portion
for confirming delivery.

### I have a Messages Module in my Runtime, but I Want to Reject all Outbound Messages. What shall I do?

You should be looking at the `bp_messages::source_chain::ForbidOutboundMessages` structure
[`bp_messages::source_chain`](../../primitives/messages/src/source_chain.rs). It implements all required traits and will
simply reject all transactions, related to outbound messages.

### How to plug-in Messages Module to Receive Messages from the Bridged Chain?

The `pallet_bridge_messages::Config` trait has 2 main associated types that are used to work with inbound messages. The
`pallet_bridge_messages::Config::SourceHeaderChain` defines how we see the bridged chain as the source of our inbound
messages. When relayer sends us a delivery transaction, this implementation must be able to parse and verify the proof
of messages wrapped in this transaction. Normally, you would reuse the same (configurable) type on all chains that are
sending messages to the same bridged chain.

The `pallet_bridge_messages::Config::MessageDispatch` defines a way on how to dispatch delivered messages. Apart from
actually dispatching the message, the implementation must return the correct dispatch weight of the message before
dispatch is called.

### I have a Messages Module in my Runtime, but I Want to Reject all Inbound Messages. What shall I do?

You should be looking at the `bp_messages::target_chain::ForbidInboundMessages` structure from the
[`bp_messages::target_chain`](../../primitives/messages/src/target_chain.rs) module. It implements all required traits
and will simply reject all transactions, related to inbound messages.

### What about other Constants in the Messages Module Configuration Trait?

Two settings that are used to check messages in the `send_message()` function. The
`pallet_bridge_messages::Config::ActiveOutboundLanes` is an array of all message lanes, that may be used to send
messages. All messages sent using other lanes are rejected. All messages that have size above
`pallet_bridge_messages::Config::MaximalOutboundPayloadSize` will also be rejected.

To be able to reward the relayer for delivering messages, we store a map of message nonces range => identifier of the
relayer that has delivered this range at the target chain runtime storage. If a relayer delivers multiple consequent
ranges, they're merged into single entry. So there may be more than one entry for the same relayer. Eventually, this
whole map must be delivered back to the source chain to confirm delivery and pay rewards. So to make sure we are able to
craft this confirmation transaction, we need to: (1) keep the size of this map below a certain limit and (2) make sure
that the weight of processing this map is below a certain limit. Both size and processing weight mostly depend on the
number of entries. The number of entries is limited with the
`pallet_bridge_messages::ConfigMaxUnrewardedRelayerEntriesAtInboundLane` parameter. Processing weight also depends on
the total number of messages that are being confirmed, because every confirmed message needs to be read. So there's
another `pallet_bridge_messages::Config::MaxUnconfirmedMessagesAtInboundLane` parameter for that.

When choosing values for these parameters, you must also keep in mind that if proof in your scheme is based on finality
of headers (and it is the most obvious option for Substrate-based chains with finality notion), then choosing too small
values for these parameters may cause significant delays in message delivery. That's because there are too many actors
involved in this scheme: 1) authorities that are finalizing headers of the target chain need to finalize header with
non-empty map; 2) the headers relayer then needs to submit this header and its finality proof to the source chain; 3)
the messages relayer must then send confirmation transaction (storage proof of this map) to the source chain; 4) when
the confirmation transaction will be mined at some header, source chain authorities must finalize this header; 5) the
headers relay then needs to submit this header and its finality proof to the target chain; 6) only now the messages
relayer may submit new messages from the source to target chain and prune the entry from the map.

Delivery transaction requires the relayer to provide both number of entries and total number of messages in the map.
This means that the module never charges an extra cost for delivering a map - the relayer would need to pay exactly for
the number of entries+messages it has delivered. So the best guess for values of these parameters would be the pair that
would occupy `N` percent of the maximal transaction size and weight of the source chain. The `N` should be large enough
to process large maps, at the same time keeping reserve for future source chain upgrades.

## Non-Essential Functionality

There may be a special account in every runtime where the messages module is deployed. This account, named 'module
owner', is like a module-level sudo account - he's able to halt and resume all module operations without requiring
runtime upgrade. Calls that are related to this account are:
- `fn set_owner()`: current module owner may call it to transfer "ownership" to another account;
- `fn halt_operations()`: the module owner (or sudo account) may call this function to stop all module operations. After
  this call, all message-related transactions will be rejected until further `resume_operations` call'. This call may be
  used when something extraordinary happens with the bridge;
- `fn resume_operations()`: module owner may call this function to resume bridge operations. The module will resume its
  regular operations after this call.

If pallet owner is not defined, the governance may be used to make those calls.

## Messages Relay

We have an offchain actor, who is watching for new messages and submits them to the bridged chain. It is the messages
relay - you may look at the [crate level documentation and the code](../../relays/messages/).
