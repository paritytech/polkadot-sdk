# Components

## BridgeHub

The system BridgeHub parachain hosts various bridges to other chains, including Ethereum and Kusama.

### InboundQueue

This [pallet](https://github.com/Snowfork/snowbridge/tree/main/parachain/pallets/inbound-queue) is responsible for accepting inbound messages from Ethereum. This involves the following:

1. Verifying that message[^1] that was included in the finalized Ethereum execution chain as tracked by our ethereum light client.
2. Converting the message to an [XCM](https://wiki.polkadot.network/docs/learn-xcm) script.
3. Sending the the XCM script to the destination parachain.

### OutboundQueue

This [pallet](https://github.com/Snowfork/snowbridge/tree/main/parachain/pallets/outbound-queue) is responsible for accepting outbound XCM messages to Ethereum. This involves the following:

1. Buffering the message in the [MessageQueue](https://github.com/paritytech/substrate/tree/master/frame/message-queue) pallet until as such as there is enough free weight in a future block to be able to process it.
2. When an XCM message is processed, it is assigned a nonce (sequence number), and converted to a simpler command which is more efficient to execute.
3. At the end of every block, a merkle root of all processed messages is generated and inserted into the parachain header as a [digest item](https://github.com/paritytech/substrate/blob/46136f2a18780d71542ae615565703da754b5348/primitives/runtime/src/generic/digest.rs#L100).
4. Processed messages are also temporarily held in storage so that they can be queried by offchain message relayers.

The merkle root in (3) is the commitment that needs to verified on the Ethereum side.

### EthereumBeaconClient

This [pallet](https://github.com/Snowfork/snowbridge/tree/main/parachain/pallets/ethereum-beacon-client) implements a light client that tracks Ethereum's [Beacon Chain](https://ethereum.org/en/roadmap/beacon-chain/). It is used to verify inbound messages submitted to the [InboundQueue](components.md#inboundqueue) pallet.

### System

This [pallet](https://github.com/Snowfork/snowbridge/tree/main/parachain/pallets/system) has overall responsibility for the bridge as well as providing basic system functionality for bridge operations.

## Ethereum

### Gateway

The Ethereum side of the bridge is organised around a central gateway [contract](../../contracts/src/interfaces/IGateway.sol), responsible for the following:

* Receiving, verifying, and dispatching inbound messages from Polkadot
* Accepting outbound messages for delivery to Polkadot
* Higher-level application features such as token transfers

### Agent

Instances of the Agent contract act as proxies for consensus systems in Polkadot.

More concretely, they have a number of purposes in the bridge design:

* When an ethereum user wishes to transfer ERC20 tokens over the bridge to the AssetHub parachain, the tokens are actually deposited into the agent instance corresponding to AssetHub.
* When a Polkadot parachain sends a general-purpose message to a Solidity contract, on the Ethereum side, the message will be dispatched to the destination contract from the Agent instance corresponding to the origin parachain.
* Offchain message relayers are incentivized by a fees & rewards system.
  * Users wanting to send outbound messages to Polkadot need to pay fees into the agent contract corresponding to the destination parachain
  * Relayers submitting messages to the Gateway are rewarded from the agent contract corresponding to the origin parachain.

The creation of new agents can be initiated permissionlessly by calling `EthereumSystem::create_agent` extrinsic on the BridgeHub parachain.

### BeefyClient

Implements a light client for verifying Polkadot Consensus. See [Polkadot Verification](verification/polkadot/) for more details.

[^1]: An inbound message is an event log emitted by our main Gateway contract on Ethereum.
