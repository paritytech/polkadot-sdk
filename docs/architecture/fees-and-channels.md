# Channels

Bridge messages flow across the bridge through logical _channels_. Each parachain that wishes to directly send or receive messages is allocated its own dedicated channel, and has some influence over the operation of its channel.

This design ensures the the following:

* Parachain governance assumes the responsibility of [rebalancing](fees-and-channels.md#rebalancing)
* The potential for custom fee/reward models on a per-channel basis
* Minimises potential [head-of-line blocking](https://en.wikipedia.org/wiki/Head-of-line\_blocking)

## Channels API

* [create\_channel](https://github.com/Snowfork/snowbridge/blob/c2142e41b5a2cbd3749a5fd8f22a95abf2b923d9/parachain/pallets/system/src/lib.rs#L410) - Create a new channel with an initial configuration
* [update\_channel](https://github.com/Snowfork/snowbridge/blob/c2142e41b5a2cbd3749a5fd8f22a95abf2b923d9/parachain/pallets/system/src/lib.rs#L443) - Update an existing channel with a new configuration

These extrinsics must be called via `Xcm::Transact` from the parachain wishing to create the channel.

As a prerequisite, the parachain must already have an agent instantiated on Ethereum. This can be done by calling [create\_agent](https://github.com/Snowfork/snowbridge/blob/c2142e41b5a2cbd3749a5fd8f22a95abf2b923d9/parachain/pallets/system/src/lib.rs#L375) via `Xcm::Transact`

## Fees & Rewards

### Ethereum -> BridgeHub -> Parachain

* On Ethereum, collected fees are deposited into the agent contract acting as a proxy for the destination parachain
* When the messages are relayed to BridgeHub, the message relayers are rewarded from the _sovereign account_ of the destination parachain
* The message is then forwarded to the final destination parachain

The net result is that:

* On Ethereum, the agent contract of the origin parachain is _credited_ with fees that cover the cost of delivery to the the destination parachain.
* On BridgeHub, sovereign account of the origin parachain is _debited_ with the costs incurred for delivery.

### Parachain -> BridgeHub -> Ethereum

* The parachain or a nested consensus system sends an XCM to BridgeHub, including `ReserveAssetDeposited` and `BuyExecution` instructions to cover the delivery fees for the `ExportMessage` instruction.
* BridgeHub calculates the cost of processing the `ExportMessage` instruction, which is divided into local and remote costs respectively. The `BuyExecution` should cover these costs. However, BridgeHub will refund the remote costs to the sovereign account of the origin parachain.
* When the message reaches Ethereum, the message relayers will be refunded and rewarded from the agent contract representing the origin parachain.

The net result is that:

* On BridgeHub, sovereign account of the origin parachain is _credited_ with fees that cover the cost of delivery to Ethereum.
* On Ethereum, the agent contract of the origin parachain is _debited_ with the costs incurred for delivery.

### Rebalancing

In both of the scenarios above, there is a common pattern:

* Collected fees are _credited_ to an account controlled by the parachain on the source network
* Costs are _debited_ from an account controlled by the parachain on the destination network&#x20;

Parachain governance therefore has the responsibility to ensure that it has enough funds to cover costs on the destination network.

This can be done by selling collected fees on the source network for currency of the destination network. This currently a manual process, but should only need to be done a few times a year.

Parachains can use the BridgeHub [transfer\_native\_from\_agent](https://github.com/Snowfork/snowbridge/blob/c2142e41b5a2cbd3749a5fd8f22a95abf2b923d9/parachain/pallets/system/src/lib.rs#L503C10-L503C36) API to transfer funds from their agent to some EOA account.

### Fee Calculation

Users pay fees in the native currency of the source chain. This fee is calculated by taking the delivery cost in the native currency of the foreign chain, and applying an exchange rate.

Currently, the exchange rates are fixed and need to be periodically updated by governance. While this is less price-efficient than using price feeds from oracles, it does ensure that the fee/rewards system is decentralized. &#x20;



