// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Instructions for XCM v5.

use bounded_collections::BoundedVec;
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use educe::Educe;

use crate::DoubleEncoded;
use crate::v5::{
	Asset, AssetFilter, AssetTransferFilter, Assets, Error, Hint, HintNumVariants,
	InteriorLocation, Junction, Location, MaybeErrorCode, NetworkId, OriginKind, QueryId,
	QueryResponseInfo, Response, Weight, WeightLimit, Xcm,
};

// TODO: group instructions by type

/// Withdraw asset(s) (`assets`) from the ownership of `origin` and place them into the Holding
/// Register.
///
/// - `assets`: The asset(s) to be withdrawn into holding.
///
/// Kind: *Command*.
///
/// Errors:
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct WithdrawAsset(pub Assets);

/// Asset(s) (`assets`) have been received into the ownership of this system on the `origin`
/// system and equivalent derivatives should be placed into the Holding Register.
///
/// - `assets`: The asset(s) that are minted into holding.
///
/// Safety: `origin` must be trusted to have received and be storing `assets` such that they
/// may later be withdrawn should this system send a corresponding message.
///
/// Kind: *Trusted Indication*.
///
/// Errors:
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct ReserveAssetDeposited(pub Assets);

/// Asset(s) (`assets`) have been destroyed on the `origin` system and equivalent assets should
/// be created and placed into the Holding Register.
///
/// - `assets`: The asset(s) that are minted into the Holding Register.
///
/// Safety: `origin` must be trusted to have irrevocably destroyed the corresponding `assets`
/// prior as a consequence of sending this message.
///
/// Kind: *Trusted Indication*.
///
/// Errors:
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct ReceiveTeleportedAsset(pub Assets);

/// Respond with information that the local system is expecting.
///
/// - `query_id`: The identifier of the query that resulted in this message being sent.
/// - `response`: The message content.
/// - `max_weight`: The maximum weight that handling this response should take.
/// - `querier`: The location responsible for the initiation of the response, if there is one.
///   In general this will tend to be the same location as the receiver of this message. NOTE:
///   As usual, this is interpreted from the perspective of the receiving consensus system.
///
/// Safety: Since this is information only, there are no immediate concerns. However, it should
/// be remembered that even if the Origin behaves reasonably, it can always be asked to make
/// a response to a third-party chain who may or may not be expecting the response. Therefore
/// the `querier` should be checked to match the expected value.
///
/// Kind: *Information*.
///
/// Errors:
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct QueryResponse {
	#[codec(compact)]
	pub query_id: u64,
	pub response: Response,
	pub max_weight: Weight,
	pub querier: Option<Location>,
}

/// Withdraw asset(s) (`assets`) from the ownership of `origin` and place equivalent assets
/// under the ownership of `beneficiary`.
///
/// - `assets`: The asset(s) to be withdrawn.
/// - `beneficiary`: The new owner for the assets.
///
/// Safety: No concerns.
///
/// Kind: *Command*.
///
/// Errors:
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct TransferAsset {
	pub assets: Assets,
	pub beneficiary: Location,
}

/// Withdraw asset(s) (`assets`) from the ownership of `origin` and place equivalent assets
/// under the ownership of `dest` within this consensus system (i.e. its sovereign account).
///
/// Send an onward XCM message to `dest` of `ReserveAssetDeposited` with the given
/// `xcm`.
///
/// - `assets`: The asset(s) to be withdrawn.
/// - `dest`: The location whose sovereign account will own the assets and thus the effective
///   beneficiary for the assets and the notification target for the reserve asset deposit
///   message.
/// - `xcm`: The instructions that should follow the `ReserveAssetDeposited` instruction, which
///   is sent onwards to `dest`.
///
/// Safety: No concerns.
///
/// Kind: *Command*.
///
/// Errors:
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct TransferReserveAsset {
	pub assets: Assets,
	pub dest: Location,
	pub xcm: Xcm<()>,
}

/// Apply the encoded transaction `call`, whose dispatch-origin should be `origin` as expressed
/// by the kind of origin `origin_kind`.
///
/// The Transact Status Register is set according to the result of dispatching the call.
///
/// - `origin_kind`: The means of expressing the message origin as a dispatch origin.
/// - `call`: The encoded transaction to be applied.
/// - `fallback_max_weight`: Used for compatibility with previous versions. Corresponds to the
///   `require_weight_at_most` parameter in previous versions. If you don't care about
///   compatibility you can just put `None`. WARNING: If you do, your XCM might not work with
///   older versions. Make sure to dry-run and validate.
///
/// Safety: No concerns.
///
/// Kind: *Command*.
///
/// Errors:
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
#[scale_info(skip_type_params(Call))]
pub struct Transact<Call> {
	pub origin_kind: OriginKind,
	pub fallback_max_weight: Option<Weight>,
	pub call: DoubleEncoded<Call>,
}

impl<Call> Transact<Call> {
	pub fn into<C>(self) -> Transact<C> {
		Transact::from(self)
	}

	pub fn from<C>(xcm: Transact<C>) -> Self {
		Self {
			origin_kind: xcm.origin_kind,
			fallback_max_weight: xcm.fallback_max_weight,
			call: xcm.call.into(),
		}
	}
}

/// A message to notify about a new incoming HRMP channel. This message is meant to be sent by
/// the relay-chain to a para.
///
/// - `sender`: The sender in the to-be opened channel. Also, the initiator of the channel
///   opening.
/// - `max_message_size`: The maximum size of a message proposed by the sender.
/// - `max_capacity`: The maximum number of messages that can be queued in the channel.
///
/// Safety: The message should originate directly from the relay-chain.
///
/// Kind: *System Notification*
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct HrmpNewChannelOpenRequest {
	#[codec(compact)]
	pub sender: u32,
	#[codec(compact)]
	pub max_message_size: u32,
	#[codec(compact)]
	pub max_capacity: u32,
}

/// A message to notify about that a previously sent open channel request has been accepted by
/// the recipient. That means that the channel will be opened during the next relay-chain
/// session change. This message is meant to be sent by the relay-chain to a para.
///
/// Safety: The message should originate directly from the relay-chain.
///
/// Kind: *System Notification*
///
/// Errors:
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct HrmpChannelAccepted {
	// NOTE: We keep this as a structured item to a) keep it consistent with the other Hrmp
	// items; and b) because the field's meaning is not obvious/mentioned from the item name.
	#[codec(compact)]
	pub recipient: u32,
}

/// A message to notify that the other party in an open channel decided to close it. In
/// particular, `initiator` is going to close the channel opened from `sender` to the
/// `recipient`. The close will be enacted at the next relay-chain session change. This message
/// is meant to be sent by the relay-chain to a para.
///
/// Safety: The message should originate directly from the relay-chain.
///
/// Kind: *System Notification*
///
/// Errors:
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct HrmpChannelClosing {
	#[codec(compact)]
	pub initiator: u32,
	#[codec(compact)]
	pub sender: u32,
	#[codec(compact)]
	pub recipient: u32,
}

/// Clear the origin.
///
/// This may be used by the XCM author to ensure that later instructions cannot command the
/// authority of the origin (e.g. if they are being relayed from an untrusted source, as often
/// the case with `ReserveAssetDeposited`).
///
/// Safety: No concerns.
///
/// Kind: *Command*.
///
/// Errors:
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct ClearOrigin;

/// Mutate the origin to some interior location.
///
/// Kind: *Command*
///
/// Errors:
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct DescendOrigin(pub InteriorLocation);

/// Immediately report the contents of the Error Register to the given destination via XCM.
///
/// A `QueryResponse` message of type `ExecutionOutcome` is sent to the described destination.
///
/// - `response_info`: Information for making the response.
///
/// Kind: *Command*
///
/// Errors:
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct ReportError(pub QueryResponseInfo);

/// Remove the asset(s) (`assets`) from the Holding Register and place equivalent assets under
/// the ownership of `beneficiary` within this consensus system.
///
/// - `assets`: The asset(s) to remove from holding.
/// - `beneficiary`: The new owner for the assets.
///
/// Kind: *Command*
///
/// Errors:
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct DepositAsset {
	pub assets: AssetFilter,
	pub beneficiary: Location,
}

/// Remove the asset(s) (`assets`) from the Holding Register and place equivalent assets under
/// the ownership of `dest` within this consensus system (i.e. deposit them into its sovereign
/// account).
///
/// Send an onward XCM message to `dest` of `ReserveAssetDeposited` with the given `effects`.
///
/// - `assets`: The asset(s) to remove from holding.
/// - `dest`: The location whose sovereign account will own the assets and thus the effective
///   beneficiary for the assets and the notification target for the reserve asset deposit
///   message.
/// - `xcm`: The orders that should follow the `ReserveAssetDeposited` instruction which is
///   sent onwards to `dest`.
///
/// Kind: *Command*
///
/// Errors:
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct DepositReserveAsset {
	pub assets: AssetFilter,
	pub dest: Location,
	pub xcm: Xcm<()>,
}

/// Remove the asset(s) (`want`) from the Holding Register and replace them with alternative
/// assets.
///
/// The minimum amount of assets to be received into the Holding Register for the order not to
/// fail may be stated.
///
/// - `give`: The maximum amount of assets to remove from holding.
/// - `want`: The minimum amount of assets which `give` should be exchanged for.
/// - `maximal`: If `true`, then prefer to give as much as possible up to the limit of `give`
///   and receive accordingly more. If `false`, then prefer to give as little as possible in
///   order to receive as little as possible while receiving at least `want`.
///
/// Kind: *Command*
///
/// Errors:
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct ExchangeAsset {
	pub give: AssetFilter,
	pub want: Assets,
	pub maximal: bool,
}

/// Remove the asset(s) (`assets`) from holding and send a `WithdrawAsset` XCM message to a
/// reserve location.
///
/// - `assets`: The asset(s) to remove from holding.
/// - `reserve`: A valid location that acts as a reserve for all asset(s) in `assets`. The
///   sovereign account of this consensus system *on the reserve location* will have
///   appropriate assets withdrawn and `effects` will be executed on them. There will typically
///   be only one valid location on any given asset/chain combination.
/// - `xcm`: The instructions to execute on the assets once withdrawn *on the reserve
///   location*.
///
/// Kind: *Command*
///
/// Errors:
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct InitiateReserveWithdraw {
	pub assets: AssetFilter,
	pub reserve: Location,
	pub xcm: Xcm<()>,
}

/// Remove the asset(s) (`assets`) from holding and send a `ReceiveTeleportedAsset` XCM message
/// to a `dest` location.
///
/// - `assets`: The asset(s) to remove from holding.
/// - `dest`: A valid location that respects teleports coming from this location.
/// - `xcm`: The instructions to execute on the assets once arrived *on the destination
///   location*.
///
/// NOTE: The `dest` location *MUST* respect this origin as a valid teleportation origin for
/// all `assets`. If it does not, then the assets may be lost.
///
/// Kind: *Command*
///
/// Errors:
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct InitiateTeleport {
	pub assets: AssetFilter,
	pub dest: Location,
	pub xcm: Xcm<()>,
}

/// Report to a given destination the contents of the Holding Register.
///
/// A `QueryResponse` message of type `Assets` is sent to the described destination.
///
/// - `response_info`: Information for making the response.
/// - `assets`: A filter for the assets that should be reported back. The assets reported back
///   will be, asset-wise, *the lesser of this value and the holding register*. No wildcards
///   will be used when reporting assets back.
///
/// Kind: *Command*
///
/// Errors:
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct ReportHolding {
	pub response_info: QueryResponseInfo,
	pub assets: AssetFilter,
}

/// Pay for the execution of some XCM `xcm` and `orders` with up to `weight`
/// picoseconds of execution time, paying for this with up to `fees` from the Holding Register.
///
/// - `fees`: The asset(s) to remove from the Holding Register to pay for fees.
/// - `weight_limit`: The maximum amount of weight to purchase; this must be at least the
///   expected maximum weight of the total XCM to be executed for the
///   `AllowTopLevelPaidExecutionFrom` barrier to allow the XCM be executed.
///
/// Kind: *Command*
///
/// Errors:
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct BuyExecution {
	pub fees: Asset,
	pub weight_limit: WeightLimit,
}

/// Refund any surplus weight previously bought with `BuyExecution`.
///
/// Kind: *Command*
///
/// Errors: None.
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct RefundSurplus;

/// Set the Error Handler Register. This is code that should be called in the case of an error
/// happening.
///
/// An error occurring within execution of this code will _NOT_ result in the error register
/// being set, nor will an error handler be called due to it. The error handler and appendix
/// may each still be set.
///
/// The apparent weight of this instruction is inclusive of the inner `Xcm`; the executing
/// weight however includes only the difference between the previous handler and the new
/// handler, which can reasonably be negative, which would result in a surplus.
///
/// Kind: *Command*
///
/// Errors: None.
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
#[scale_info(skip_type_params(Call))]
pub struct SetErrorHandler<Call>(pub Xcm<Call>);

impl<Call> SetErrorHandler<Call> {
	pub fn into<C>(self) -> SetErrorHandler<C> {
		SetErrorHandler::from(self)
	}

	pub fn from<C>(xcm: SetErrorHandler<C>) -> Self {
		Self(xcm.0.into())
	}
}

/// Set the Appendix Register. This is code that should be called after code execution
/// (including the error handler if any) is finished. This will be called regardless of whether
/// an error occurred.
///
/// Any error occurring due to execution of this code will result in the error register being
/// set, and the error handler (if set) firing.
///
/// The apparent weight of this instruction is inclusive of the inner `Xcm`; the executing
/// weight however includes only the difference between the previous appendix and the new
/// appendix, which can reasonably be negative, which would result in a surplus.
///
/// Kind: *Command*
///
/// Errors: None.
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
#[scale_info(skip_type_params(Call))]
pub struct SetAppendix<Call>(pub Xcm<Call>);

impl<Call> SetAppendix<Call> {
	pub fn into<C>(self) -> SetAppendix<C> {
		SetAppendix::from(self)
	}

	pub fn from<C>(xcm: SetAppendix<C>) -> Self {
		Self(xcm.0.into())
	}
}

/// Clear the Error Register.
///
/// Kind: *Command*
///
/// Errors: None.
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct ClearError;

/// Create some assets which are being held on behalf of the origin.
///
/// - `assets`: The assets which are to be claimed. This must match exactly with the assets
///   claimable by the origin of the ticket.
/// - `ticket`: The ticket of the asset; this is an abstract identifier to help locate the
///   asset.
///
/// Kind: *Command*
///
/// Errors:
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct ClaimAsset {
	pub assets: Assets,
	pub ticket: Location,
}

/// Always throws an error of type `Trap`.
///
/// Kind: *Command*
///
/// Errors:
/// - `Trap`: All circumstances, whose inner value is the same as this item's inner value.
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct Trap(#[codec(compact)] pub u64);

/// Ask the destination system to respond with the most recent version of XCM that they
/// support in a `QueryResponse` instruction. Any changes to this should also elicit similar
/// responses when they happen.
///
/// - `query_id`: An identifier that will be replicated into the returned XCM message.
/// - `max_response_weight`: The maximum amount of weight that the `QueryResponse` item which
///   is sent as a reply may take to execute. NOTE: If this is unexpectedly large then the
///   response may not execute at all.
///
/// Kind: *Command*
///
/// Errors: *Fallible*
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct SubscribeVersion {
	#[codec(compact)]
	pub query_id: QueryId,
	pub max_response_weight: Weight,
}

/// Cancel the effect of a previous `SubscribeVersion` instruction.
///
/// Kind: *Command*
///
/// Errors: *Fallible*
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct UnsubscribeVersion;

/// Reduce Holding by up to the given assets.
///
/// Holding is reduced by as much as possible up to the assets in the parameter. It is not an
/// error if the Holding does not contain the assets (to make this an error, use `ExpectAsset`
/// prior).
///
/// Kind: *Command*
///
/// Errors: *Infallible*
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct BurnAsset(pub Assets);

/// Throw an error if Holding does not contain at least the given assets.
///
/// Kind: *Command*
///
/// Errors:
/// - `ExpectationFalse`: If Holding Register does not contain the assets in the parameter.
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct ExpectAsset(pub Assets);

/// Ensure that the Origin Register equals some given value and throw an error if not.
///
/// Kind: *Command*
///
/// Errors:
/// - `ExpectationFalse`: If Origin Register is not equal to the parameter.
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct ExpectOrigin(pub Option<Location>);

/// Ensure that the Error Register equals some given value and throw an error if not.
///
/// Kind: *Command*
///
/// Errors:
/// - `ExpectationFalse`: If the value of the Error Register is not equal to the parameter.
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct ExpectError(pub Option<(u32, Error)>);

/// Ensure that the Transact Status Register equals some given value and throw an error if
/// not.
///
/// Kind: *Command*
///
/// Errors:
/// - `ExpectationFalse`: If the value of the Transact Status Register is not equal to the
///   parameter.
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct ExpectTransactStatus(pub MaybeErrorCode);

/// Query the existence of a particular pallet type.
///
/// - `module_name`: The module name of the pallet to query.
/// - `response_info`: Information for making the response.
///
/// Sends a `QueryResponse` to Origin whose data field `PalletsInfo` containing the information
/// of all pallets on the local chain whose name is equal to `name`. This is empty in the case
/// that the local chain is not based on Substrate Frame.
///
/// Safety: No concerns.
///
/// Kind: *Command*
///
/// Errors: *Fallible*.
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct QueryPallet {
	pub module_name: Vec<u8>,
	pub response_info: QueryResponseInfo,
}

/// Ensure that a particular pallet with a particular version exists.
///
/// - `index: Compact`: The index which identifies the pallet. An error if no pallet exists at
///   this index.
/// - `name: Vec<u8>`: Name which must be equal to the name of the pallet.
/// - `module_name: Vec<u8>`: Module name which must be equal to the name of the module in
///   which the pallet exists.
/// - `crate_major: Compact`: Version number which must be equal to the major version of the
///   crate which implements the pallet.
/// - `min_crate_minor: Compact`: Version number which must be at most the minor version of the
///   crate which implements the pallet.
///
/// Safety: No concerns.
///
/// Kind: *Command*
///
/// Errors:
/// - `ExpectationFalse`: In case any of the expectations are broken.
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct ExpectPallet {
	#[codec(compact)]
	pub index: u32,
	pub name: Vec<u8>,
	pub module_name: Vec<u8>,
	#[codec(compact)]
	pub crate_major: u32,
	#[codec(compact)]
	pub min_crate_minor: u32,
}

/// Send a `QueryResponse` message containing the value of the Transact Status Register to some
/// destination.
///
/// - `query_response_info`: The information needed for constructing and sending the
///   `QueryResponse` message.
///
/// Safety: No concerns.
///
/// Kind: *Command*
///
/// Errors: *Fallible*.
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct ReportTransactStatus(pub QueryResponseInfo);

/// Set the Transact Status Register to its default, cleared, value.
///
/// Safety: No concerns.
///
/// Kind: *Command*
///
/// Errors: *Infallible*.
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct ClearTransactStatus;

/// Set the Origin Register to be some child of the Universal Ancestor.
///
/// Safety: Should only be usable if the Origin is trusted to represent the Universal Ancestor
/// child in general. In general, no Origin should be able to represent the Universal Ancestor
/// child which is the root of the local consensus system since it would by extension
/// allow it to act as any location within the local consensus.
///
/// The `Junction` parameter should generally be a `GlobalConsensus` variant since it is only
/// these which are children of the Universal Ancestor.
///
/// Kind: *Command*
///
/// Errors: *Fallible*.
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct UniversalOrigin(pub Junction);

/// Send a message on to Non-Local Consensus system.
///
/// This will tend to utilize some extra-consensus mechanism, the obvious one being a bridge.
/// A fee may be charged; this may be determined based on the contents of `xcm`. It will be
/// taken from the Holding register.
///
/// - `network`: The remote consensus system to which the message should be exported.
/// - `destination`: The location relative to the remote consensus system to which the message
///   should be sent on arrival.
/// - `xcm`: The message to be exported.
///
/// As an example, to export a message for execution on Statemine (parachain #1000 in the
/// Kusama network), you would call with `network: NetworkId::Kusama` and
/// `destination: [Parachain(1000)].into()`. Alternatively, to export a message for execution
/// on Polkadot, you would call with `network: NetworkId:: Polkadot` and `destination: Here`.
///
/// Kind: *Command*
///
/// Errors: *Fallible*.
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct ExportMessage {
	pub network: NetworkId,
	pub destination: InteriorLocation,
	pub xcm: Xcm<()>,
}

/// Lock the locally held asset and prevent further transfer or withdrawal.
///
/// This restriction may be removed by the `UnlockAsset` instruction being called with an
/// Origin of `unlocker` and a `target` equal to the current `Origin`.
///
/// If the locking is successful, then a `NoteUnlockable` instruction is sent to `unlocker`.
///
/// - `asset`: The asset(s) which should be locked.
/// - `unlocker`: The value which the Origin must be for a corresponding `UnlockAsset`
///   instruction to work.
///
/// Kind: *Command*.
///
/// Errors:
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct LockAsset {
	pub asset: Asset,
	pub unlocker: Location,
}

/// Remove the lock over `asset` on this chain and (if nothing else is preventing it) allow the
/// asset to be transferred.
///
/// - `asset`: The asset to be unlocked.
/// - `target`: The owner of the asset on the local chain.
///
/// Safety: No concerns.
///
/// Kind: *Command*.
///
/// Errors:
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct UnlockAsset {
	pub asset: Asset,
	pub target: Location,
}

/// Asset (`asset`) has been locked on the `origin` system and may not be transferred. It may
/// only be unlocked with the receipt of the `UnlockAsset` instruction from this chain.
///
/// - `asset`: The asset(s) which are now unlockable from this origin.
/// - `owner`: The owner of the asset on the chain in which it was locked. This may be a
///   location specific to the origin network.
///
/// Safety: `origin` must be trusted to have locked the corresponding `asset`
/// prior as a consequence of sending this message.
///
/// Kind: *Trusted Indication*.
///
/// Errors:
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct NoteUnlockable {
	pub asset: Asset,
	pub owner: Location,
}

/// Send an `UnlockAsset` instruction to the `locker` for the given `asset`.
///
/// This may fail if the local system is making use of the fact that the asset is locked or,
/// of course, if there is no record that the asset actually is locked.
///
/// - `asset`: The asset(s) to be unlocked.
/// - `locker`: The location from which a previous `NoteUnlockable` was sent and to which an
///   `UnlockAsset` should be sent.
///
/// Kind: *Command*.
///
/// Errors:
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct RequestUnlock {
	pub asset: Asset,
	pub locker: Location,
}

/// Sets the Fees Mode Register.
///
/// - `jit_withdraw`: The fees mode item; if set to `true` then fees for any instructions are
///   withdrawn as needed using the same mechanism as `WithdrawAssets`.
///
/// Kind: *Command*.
///
/// Errors:
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct SetFeesMode {
	pub jit_withdraw: bool,
}

/// Set the Topic Register.
///
/// The 32-byte array identifier in the parameter is not guaranteed to be
/// unique; if such a property is desired, it is up to the code author to
/// enforce uniqueness.
///
/// Safety: No concerns.
///
/// Kind: *Command*
///
/// Errors:
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct SetTopic(pub [u8; 32]);

/// Clear the Topic Register.
///
/// Kind: *Command*
///
/// Errors: None.
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct ClearTopic;

/// Alter the current Origin to another given origin.
///
/// Kind: *Command*
///
/// Errors: If the existing state would not allow such a change.
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct AliasOrigin(pub Location);

/// A directive to indicate that the origin expects free execution of the message.
///
/// At execution time, this instruction just does a check on the Origin register.
/// However, at the barrier stage messages starting with this instruction can be disregarded if
/// the origin is not acceptable for free execution or the `weight_limit` is `Limited` and
/// insufficient.
///
/// Kind: *Indication*
///
/// Errors: If the given origin is `Some` and not equal to the current Origin register.
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct UnpaidExecution {
	pub weight_limit: WeightLimit,
	pub check_origin: Option<Location>,
}

/// Pay Fees.
///
/// Successor to `BuyExecution`.
/// Defined in fellowship RFC 105.
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct PayFees {
	pub asset: Asset,
}

/// Initiates cross-chain transfer as follows:
///
/// Assets in the holding register are matched using the given list of `AssetTransferFilter`s,
/// they are then transferred based on their specified transfer type:
///
/// - teleport: burn local assets and append a `ReceiveTeleportedAsset` XCM instruction to the
///   XCM program to be sent onward to the `destination` location,
///
/// - reserve deposit: place assets under the ownership of `destination` within this consensus
///   system (i.e. its sovereign account), and append a `ReserveAssetDeposited` XCM instruction
///   to the XCM program to be sent onward to the `destination` location,
///
/// - reserve withdraw: burn local assets and append a `WithdrawAsset` XCM instruction to the
///   XCM program to be sent onward to the `destination` location,
///
/// The onward XCM is then appended a `ClearOrigin` to allow safe execution of any following
/// custom XCM instructions provided in `remote_xcm`.
///
/// The onward XCM also contains either a `PayFees` or `UnpaidExecution` instruction based
/// on the presence of the `remote_fees` parameter (see below).
///
/// If an XCM program requires going through multiple hops, it can compose this instruction to
/// be used at every chain along the path, describing that specific leg of the flow.
///
/// Parameters:
/// - `destination`: The location of the program next hop.
/// - `remote_fees`: If set to `Some(asset_xfer_filter)`, the single asset matching
///   `asset_xfer_filter` in the holding register will be transferred first in the remote XCM
///   program, followed by a `PayFees(fee)`, then rest of transfers follow. This guarantees
///   `remote_xcm` will successfully pass a `AllowTopLevelPaidExecutionFrom` barrier. If set to
///   `None`, a `UnpaidExecution` instruction is appended instead. Please note that these
///   assets are **reserved** for fees, they are sent to the fees register rather than holding.
///   Best practice is to only add here enough to cover fees, and transfer the rest through the
///   `assets` parameter.
/// - `preserve_origin`: Specifies whether the original origin should be preserved or cleared,
///   using the instructions `AliasOrigin` or `ClearOrigin` respectively.
/// - `assets`: List of asset filters matched against existing assets in holding. These are
///   transferred over to `destination` using the specified transfer type, and deposited to
///   holding on `destination`.
/// - `remote_xcm`: Custom instructions that will be executed on the `destination` chain. Note
///   that these instructions will be executed after a `ClearOrigin` so their origin will be
///   `None`.
///
/// Safety: No concerns.
///
/// Kind: *Command*
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct InitiateTransfer {
	pub destination: Location,
	pub remote_fees: Option<AssetTransferFilter>,
	pub preserve_origin: bool,
	pub assets: Vec<AssetTransferFilter>,
	pub remote_xcm: Xcm<()>,
}

/// Executes inner `xcm` with origin set to the provided `descendant_origin`. Once the inner
/// `xcm` is executed, the original origin (the one active for this instruction) is restored.
///
/// Parameters:
/// - `descendant_origin`: The origin that will be used during the execution of the inner
///   `xcm`. If set to `None`, the inner `xcm` is executed with no origin. If set to `Some(o)`,
///   the inner `xcm` is executed as if there was a `DescendOrigin(o)` executed before it, and
///   runs the inner xcm with origin: `original_origin.append_with(o)`.
/// - `xcm`: Inner instructions that will be executed with the origin modified according to
///   `descendant_origin`.
///
/// Safety: No concerns.
///
/// Kind: *Command*
///
/// Errors:
/// - `BadOrigin`
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
#[scale_info(skip_type_params(Call))]
pub struct ExecuteWithOrigin<Call> { // TODO: make this generic over Xcm so it is using the current version
	pub descendant_origin: Option<InteriorLocation>,
	pub xcm: Xcm<Call>,
}

impl<Call> ExecuteWithOrigin<Call> {
	pub fn into<C>(self) -> ExecuteWithOrigin<C> {
		ExecuteWithOrigin::from(self)
	}

	pub fn from<C>(xcm: ExecuteWithOrigin<C>) -> Self {
		Self {
			descendant_origin: xcm.descendant_origin,
			xcm: xcm.xcm.into(),
		}
	}
}

/// Set hints for XCM execution.
///
/// These hints change the behaviour of the XCM program they are present in.
///
/// Parameters:
///
/// - `hints`: A bounded vector of `ExecutionHint`, specifying the different hints that will
/// be activated.
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct SetHints {
	pub hints: BoundedVec<Hint, HintNumVariants>,
}
