// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! XCM configurations for the Rialto runtime.

use super::{
	millau_messages::WithMillauMessageBridge, AccountId, AllPalletsWithSystem, Balances,
	BridgeMillauMessages, Call, Event, Origin, Runtime, XcmPallet,
};
use bp_messages::source_chain::MessagesBridge;
use bp_rialto::{Balance, WeightToFee};
use bridge_runtime_common::messages::{
	source::{estimate_message_dispatch_and_delivery_fee, FromThisChainMessagePayload},
	MessageBridge,
};
use codec::Encode;
use frame_support::{
	parameter_types,
	traits::{Everything, Nothing},
	weights::Weight,
};
use sp_std::marker::PhantomData;
use xcm::latest::prelude::*;
use xcm_builder::{
	AccountId32Aliases, AllowKnownQueryResponses, AllowTopLevelPaidExecutionFrom,
	CurrencyAdapter as XcmCurrencyAdapter, IsConcrete, SignedAccountId32AsNative,
	SignedToAccountId32, SovereignSignedViaLocation, TakeWeightCredit, UsingComponents,
};

parameter_types! {
	/// The location of the `MLAU` token, from the context of this chain. Since this token is native to this
	/// chain, we make it synonymous with it and thus it is the `Here` location, which means "equivalent to
	/// the context".
	pub const TokenLocation: MultiLocation = Here.into_location();
	/// The Rialto network ID, associated with Polkadot.
	pub const ThisNetwork: NetworkId = Polkadot;
	/// The Millau network ID, associated with Kusama.
	pub const MillauNetwork: NetworkId = Kusama;

	/// Our XCM location ancestry - i.e. our location within the Consensus Universe.
	///
	/// Since Polkadot is a top-level relay-chain with its own consensus, it's just our network ID.
	pub UniversalLocation: InteriorMultiLocation = ThisNetwork::get().into();
	/// The check account, which holds any native assets that have been teleported out and not back in (yet).
	pub CheckAccount: AccountId = XcmPallet::check_account();
}

/// The canonical means of converting a `MultiLocation` into an `AccountId`, used when we want to
/// determine the sovereign account controlled by a location.
pub type SovereignAccountOf = (
	// We can directly alias an `AccountId32` into a local account.
	AccountId32Aliases<ThisNetwork, AccountId>,
);

/// Our asset transactor. This is what allows us to interest with the runtime facilities from the
/// point of view of XCM-only concepts like `MultiLocation` and `MultiAsset`.
///
/// Ours is only aware of the Balances pallet, which is mapped to `TokenLocation`.
pub type LocalAssetTransactor = XcmCurrencyAdapter<
	// Use this currency:
	Balances,
	// Use this currency when it is a fungible asset matching the given location or name:
	IsConcrete<TokenLocation>,
	// We can convert the MultiLocations with our converter above:
	SovereignAccountOf,
	// Our chain's account ID type (we can't get away without mentioning it explicitly):
	AccountId,
	// We track our teleports in/out to keep total issuance correct.
	CheckAccount,
>;

/// The means that we convert the XCM message origin location into a local dispatch origin.
type LocalOriginConverter = (
	// A `Signed` origin of the sovereign account that the original location controls.
	SovereignSignedViaLocation<SovereignAccountOf, Origin>,
	// The AccountId32 location type can be expressed natively as a `Signed` origin.
	SignedAccountId32AsNative<ThisNetwork, Origin>,
);

/// The amount of weight an XCM operation takes. This is a safe overestimate.
pub const BASE_XCM_WEIGHT: Weight = 1_000_000_000;

parameter_types! {
	/// The amount of weight an XCM operation takes. This is a safe overestimate.
	pub const BaseXcmWeight: Weight = BASE_XCM_WEIGHT;
	/// Maximum number of instructions in a single XCM fragment. A sanity check against weight
	/// calculations getting too crazy.
	pub const MaxInstructions: u32 = 100;
}

/// The XCM router. When we want to send an XCM message, we use this type. It amalgamates all of our
/// individual routers.
pub type XcmRouter = (
	// Router to send messages to Millau.
	ToMillauBridge<BridgeMillauMessages>,
);

parameter_types! {
	pub const MaxAssetsIntoHolding: u32 = 64;
}

/// The barriers one of which must be passed for an XCM message to be executed.
pub type Barrier = (
	// Weight that is paid for may be consumed.
	TakeWeightCredit,
	// If the message is one that immediately attemps to pay for execution, then allow it.
	AllowTopLevelPaidExecutionFrom<Everything>,
	// Expected responses are OK.
	AllowKnownQueryResponses<XcmPallet>,
);

/// Incoming XCM weigher type.
pub type XcmWeigher = xcm_builder::FixedWeightBounds<BaseXcmWeight, Call, MaxInstructions>;

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
	type Call = Call;
	type XcmSender = XcmRouter;
	type AssetTransactor = LocalAssetTransactor;
	type OriginConverter = LocalOriginConverter;
	type IsReserve = ();
	type IsTeleporter = ();
	type UniversalLocation = UniversalLocation;
	type Barrier = Barrier;
	type Weigher = XcmWeigher;
	// The weight trader piggybacks on the existing transaction-fee conversion logic.
	type Trader = UsingComponents<WeightToFee, TokenLocation, AccountId, Balances, ()>;
	type ResponseHandler = XcmPallet;
	type AssetTrap = XcmPallet;
	type AssetLocker = ();
	type AssetExchanger = ();
	type AssetClaims = XcmPallet;
	type SubscriptionService = XcmPallet;
	type PalletInstancesInfo = AllPalletsWithSystem;
	type MaxAssetsIntoHolding = MaxAssetsIntoHolding;
	type FeeManager = ();
	type MessageExporter = ();
	type UniversalAliases = Nothing;
}

/// Type to convert an `Origin` type value into a `MultiLocation` value which represents an interior
/// location of this chain.
pub type LocalOriginToLocation = (
	// Usual Signed origin to be used in XCM as a corresponding AccountId32
	SignedToAccountId32<Origin, AccountId, ThisNetwork>,
);

impl pallet_xcm::Config for Runtime {
	type Event = Event;
	// We don't allow any messages to be sent via the transaction yet. This is basically safe to
	// enable, (safe the possibility of someone spamming the parachain if they're willing to pay
	// the DOT to send from the Relay-chain). But it's useless until we bring in XCM v3 which will
	// make `DescendOrigin` a bit more useful.
	type SendXcmOrigin = xcm_builder::EnsureXcmOrigin<Origin, LocalOriginToLocation>;
	type XcmRouter = XcmRouter;
	// Anyone can execute XCM messages locally.
	type ExecuteXcmOrigin = xcm_builder::EnsureXcmOrigin<Origin, LocalOriginToLocation>;
	type XcmExecuteFilter = Everything;
	type XcmExecutor = xcm_executor::XcmExecutor<XcmConfig>;
	// Anyone is able to use teleportation regardless of who they are and what they want to
	// teleport.
	type XcmTeleportFilter = Everything;
	// Anyone is able to use reserve transfers regardless of who they are and what they want to
	// transfer.
	type XcmReserveTransferFilter = Everything;
	type Weigher = XcmWeigher;
	type UniversalLocation = UniversalLocation;
	type Origin = Origin;
	type Call = Call;
	const VERSION_DISCOVERY_QUEUE_SIZE: u32 = 100;
	type AdvertisedXcmVersion = pallet_xcm::CurrentXcmVersion;
	type Currency = Balances;
	type CurrencyMatcher = ();
	type TrustedLockers = ();
	type SovereignAccountOf = SovereignAccountOf;
	type MaxLockers = frame_support::traits::ConstU32<8>;
}

/// With-rialto bridge.
pub struct ToMillauBridge<MB>(PhantomData<MB>);

impl<MB: MessagesBridge<Origin, AccountId, Balance, FromThisChainMessagePayload>> SendXcm
	for ToMillauBridge<MB>
{
	type Ticket = (Balance, FromThisChainMessagePayload);

	fn validate(
		dest: &mut Option<MultiLocation>,
		msg: &mut Option<Xcm<()>>,
	) -> SendResult<Self::Ticket> {
		let d = dest.take().ok_or(SendError::MissingArgument)?;
		if !matches!(d, MultiLocation { parents: 1, interior: X1(GlobalConsensus(r)) } if r == MillauNetwork::get())
		{
			*dest = Some(d);
			return Err(SendError::NotApplicable)
		};

		let dest: InteriorMultiLocation = MillauNetwork::get().into();
		let here = UniversalLocation::get();
		let route = dest.relative_to(&here);
		let msg = (route, msg.take().unwrap()).encode();

		let fee = estimate_message_dispatch_and_delivery_fee::<WithMillauMessageBridge>(
			&msg,
			WithMillauMessageBridge::RELAYER_FEE_PERCENT,
			None,
		)
		.map_err(SendError::Transport)?;
		let fee_assets = MultiAssets::from((Here, fee));

		Ok(((fee, msg), fee_assets))
	}

	fn deliver(ticket: Self::Ticket) -> Result<XcmHash, SendError> {
		let lane = [0, 0, 0, 0];
		let (fee, msg) = ticket;
		let result = MB::send_message(
			pallet_xcm::Origin::from(MultiLocation::from(UniversalLocation::get())).into(),
			lane,
			msg,
			fee,
		);
		result
			.map(|artifacts| {
				let hash = (lane, artifacts.nonce).using_encoded(sp_io::hashing::blake2_256);
				log::debug!(target: "runtime::bridge", "Sent XCM message {:?}/{} to Rialto: {:?}", lane, artifacts.nonce, hash);
				hash
			})
			.map_err(|e| {
				log::debug!(target: "runtime::bridge", "Failed to send XCM message over lane {:?} to Rialto: {:?}", lane, e);
				SendError::Transport("Bridge has rejected the message")
			})
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use bp_messages::{
		target_chain::{DispatchMessage, DispatchMessageData, MessageDispatch},
		MessageKey,
	};
	use bp_runtime::messages::MessageDispatchResult;
	use bridge_runtime_common::messages::target::FromBridgedChainMessageDispatch;

	fn new_test_ext() -> sp_io::TestExternalities {
		sp_io::TestExternalities::new(
			frame_system::GenesisConfig::default().build_storage::<Runtime>().unwrap(),
		)
	}

	#[test]
	fn xcm_messages_to_millau_are_sent() {
		new_test_ext().execute_with(|| {
			// the encoded message (origin ++ xcm) is 0x010109030419A8
			let dest = (Parent, X1(GlobalConsensus(MillauNetwork::get())));
			let xcm: Xcm<()> = vec![Instruction::Trap(42)].into();

			let send_result = send_xcm::<XcmRouter>(dest.into(), xcm);
			let expected_fee = MultiAssets::from((Here, 4_345_002_552_u128));
			let expected_hash =
				([0u8, 0u8, 0u8, 0u8], 1u64).using_encoded(sp_io::hashing::blake2_256);
			assert_eq!(send_result, Ok((expected_hash, expected_fee)),);
		})
	}

	#[test]
	fn xcm_messages_from_millau_are_dispatched() {
		type XcmExecutor = xcm_executor::XcmExecutor<XcmConfig>;
		type MessageDispatcher = FromBridgedChainMessageDispatch<
			WithMillauMessageBridge,
			XcmExecutor,
			XcmWeigher,
			frame_support::traits::ConstU64<BASE_XCM_WEIGHT>,
		>;

		new_test_ext().execute_with(|| {
			let location: MultiLocation =
				(Parent, X1(GlobalConsensus(MillauNetwork::get()))).into();
			let xcm: Xcm<Call> = vec![Instruction::Trap(42)].into();

			let mut incoming_message = DispatchMessage {
				key: MessageKey { lane_id: [0, 0, 0, 0], nonce: 1 },
				data: DispatchMessageData { payload: Ok((location, xcm).into()), fee: 0 },
			};

			let dispatch_weight = MessageDispatcher::dispatch_weight(&mut incoming_message);
			assert_eq!(dispatch_weight, 1_000_000_000);

			let dispatch_result =
				MessageDispatcher::dispatch(&AccountId::from([0u8; 32]), incoming_message);
			assert_eq!(
				dispatch_result,
				MessageDispatchResult {
					dispatch_result: true,
					unspent_weight: 0,
					dispatch_fee_paid_during_dispatch: false,
				}
			);
		})
	}
}
