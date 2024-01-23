# Concepts

## Trustlessness <a href="#_6hddi335yfdz" id="_6hddi335yfdz"></a>

In the blockchain space, most applications are driven by or augmented with financial use cases. This means that end users are giving up some control over their finances to whatever system they use. By giving up this control, they trust that the systems they use will protect their funds and stick to the expectations they have about how the system functions.

We define a trustless system as a system in which the end user does not need to trust any participants or group of participants in the system in order to maintain protection of their funds and expectations of functionality. They only need to trust the protocols, mathematics, cryptography, code and economics.

This can be achieved in various ways. The important thing here is for safety of user funds and expectations to be preserved, irrespective of the participants in the system.

100% trustlessness cannot always be guaranteed—there always needs to be some set of basic assumptions that must be taken on. Snowbridge caters to a set of assumptions that will be mostly uncontroversial and acceptable by the community.

## General-purpose <a href="#block-bbe1e16fb6614924a360297dcea763b2" id="block-bbe1e16fb6614924a360297dcea763b2"></a>

In the interoperability and bridge space, the default thing that comes to mind for most people and projects is the transfer of tokens from one blockchain to another. Most bridges tackle this piece of functionality first and design with it in mind.

However, interoperability is about more than just token transfers. Other kinds of assets, like non-fungible tokens, loan contracts, option/future contracts and generalized, type agnostic asset transfers across chains would be valuable functionality.

Being general-purpose, Snowbridge can facilitate transfer of arbitrary state across chains through arbitrary messages that are not tied to any particular application, and can support cross-chain applications.

## Deliverability and Delivery <a href="#deliverability-and-delivery" id="deliverability-and-delivery"></a>

In the context of this documentation, we often use the words guaranteed deliverability and guaranteed delivery. They both refer to different kinds of trust in the bridge.

If a bridge has _Guaranteed Deliverability_ it means that it is trustlessly possible for a message to be delivered across that bridge, ie, so long as someone is willing to run the software to relay the message and pay gas fees, it will be processed successfully and go through. _Guaranteed Deliverability_ does not mean that someone will actually do so - only that it is possible to do so without permission.

With _Guaranteed Deliverability_, the sender of the message can always deliver the message themself if they are willing to run a relayer and pay gas prices to do so, and so does not need to trust any third party if they don’t want to.

_Guaranteed Delivery_ on the other hand means that in addition, there are strong incentives or requirements for messages to be delivered such that based on economic assumptions, some third party will actually run software to relay messages and pay for gas and so messages will in fact be delivered even if the sender does not relay themself.
