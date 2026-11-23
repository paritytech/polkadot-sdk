// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! The relay-chain backing: `ParachainSystem`, with `Aura` and `AuraExt` for consensus, and the
//! relay chain's `Coretime` pallet providing the cores the broker sells.

use alloc::{vec, vec::Vec};
use codec::{Decode, Encode};
use cumulus_pallet_parachain_system::{RelayNumberMonotonicallyIncreases, RelaychainDataProvider};
use cumulus_primitives_core::{relay_chain, AggregateMessageOrigin};
use frame_support::{
	parameter_types,
	traits::{ConstBool, ConstU32, ConstU64},
	weights::Weight,
};
use pallet_broker::{CoreAssignment, CoreIndex, CoretimeInterface, PartsOf57600, RCBlockNumberOf};
use parachains_common::{AccountId, AuraId, Balance};
use polkadot_runtime_common::xcm_sender::ExponentialPrice;
use sp_runtime::impl_opaque_keys;
use testnet_parachains_constants::westend::consensus::{
	BLOCK_PROCESSING_VELOCITY, MAXIMUM_BLOCK_WEIGHT, RELAY_CHAIN_SLOT_DURATION_MILLIS,
	SLOT_DURATION, UNINCLUDED_SEGMENT_CAPACITY,
};
use xcm::latest::prelude::*;

use crate::{
	weights, Aura, BaseDeliveryFee, FeeAssetId, MessageQueue, ParachainSystem, PolkadotXcm,
	Runtime, RuntimeEvent, TransactionByteFee, XcmpQueue, OFFSET, PERIOD,
};

impl_opaque_keys! {
	pub struct SessionKeys {
		pub aura: Aura,
	}
}

pub(crate) const RELAY_PARENT_OFFSET: u32 = 0;

pub(crate) type ConsensusHook = cumulus_pallet_aura_ext::FixedVelocityConsensusHook<
	Runtime,
	RELAY_CHAIN_SLOT_DURATION_MILLIS,
	BLOCK_PROCESSING_VELOCITY,
	UNINCLUDED_SEGMENT_CAPACITY,
>;

/// The action to take on a runtime upgrade.
pub(crate) type OnSetCode = cumulus_pallet_parachain_system::ParachainSetCode<Runtime>;

parameter_types! {
	pub const ReservedXcmpWeight: Weight = MAXIMUM_BLOCK_WEIGHT.saturating_div(4);
	pub const ReservedDmpWeight: Weight = MAXIMUM_BLOCK_WEIGHT.saturating_div(4);
	pub const RelayOrigin: AggregateMessageOrigin = AggregateMessageOrigin::Parent;
}

impl cumulus_pallet_parachain_system::Config for Runtime {
	type WeightInfo = weights::cumulus_pallet_parachain_system::WeightInfo<Runtime>;
	type RuntimeEvent = RuntimeEvent;
	type OnSystemEvent = ();
	type SelfParaId = parachain_info::Pallet<Runtime>;
	type DmpQueue = frame_support::traits::EnqueueWithOrigin<MessageQueue, RelayOrigin>;
	type OutboundXcmpMessageSource = XcmpQueue;
	type ReservedDmpWeight = ReservedDmpWeight;
	type XcmpMessageHandler = XcmpQueue;
	type ReservedXcmpWeight = ReservedXcmpWeight;
	type CheckAssociatedRelayNumber = RelayNumberMonotonicallyIncreases;
	type ConsensusHook = ConsensusHook;
	type RelayParentOffset = ConstU32<RELAY_PARENT_OFFSET>;
	type SchedulingSignatureVerifier = ();
}

/// The clock the runtime reads as "the relay block number": the relay parent number.
pub type SlotNumber = RelaychainDataProvider<Runtime>;

impl cumulus_pallet_aura_ext::Config for Runtime {}

impl pallet_aura::Config for Runtime {
	type AuthorityId = AuraId;
	type DisabledValidators = ();
	type MaxAuthorities = ConstU32<100_000>;
	type AllowMultipleBlocksPerSlot = ConstBool<true>;
	type SlotDuration = ConstU64<SLOT_DURATION>;
}

/// A session lasts `PERIOD` blocks.
pub(crate) type SessionRotation =
	pallet_session::PeriodicSessions<ConstU32<PERIOD>, ConstU32<OFFSET>>;

/// Aura checks that the timestamp is in the block's slot.
pub(crate) type OnTimestampSet = Aura;

/// The block author is the Aura authority of the block's slot.
pub(crate) type FindAuthor = pallet_session::FindAccountFromAuthorIndex<Runtime, Aura>;

/// The price for delivering XCM messages to the relay chain.
pub type PriceForParentDelivery =
	ExponentialPrice<FeeAssetId, BaseDeliveryFee, TransactionByteFee, ParachainSystem>;

/// Sends XCM to the relay chain over UMP.
pub(crate) type ParentXcmRouter =
	cumulus_primitives_utility::ParentAsUmp<ParachainSystem, PolkadotXcm, PriceForParentDelivery>;

/// The XCMP channels to sibling parachains are the ones the relay chain opened.
pub(crate) type XcmpChannelInfo = ParachainSystem;

/// The permanent migrations of the pallets this backing composes.
pub(crate) type Migrations = (
	cumulus_pallet_aura_ext::migration::MigrateV0ToV1<Runtime>,
	cumulus_pallet_parachain_system::migration::Migration<Runtime>,
);

/// The Aura authorities.
pub(crate) fn authorities() -> Vec<AuraId> {
	pallet_aura::Authorities::<Runtime>::get().into_inner()
}

/// A type containing the encoding of the coretime pallet in the Relay chain runtime. Used to
/// construct any remote calls. The codec index must correspond to the index of `Coretime` in the
/// `construct_runtime` of the Relay chain.
#[derive(Encode, Decode)]
enum RelayRuntimePallets {
	#[codec(index = 66)]
	Coretime(CoretimeProviderCalls),
}

/// Call encoding for the calls needed from the relay coretime pallet.
#[derive(Encode, Decode)]
enum CoretimeProviderCalls {
	#[codec(index = 1)]
	RequestCoreCount(CoreIndex),
	#[codec(index = 2)]
	RequestRevenueInfoAt(relay_chain::BlockNumber),
	#[codec(index = 3)]
	CreditAccount(AccountId, Balance),
	#[codec(index = 4)]
	AssignCore(
		CoreIndex,
		relay_chain::BlockNumber,
		Vec<(CoreAssignment, PartsOf57600)>,
		Option<relay_chain::BlockNumber>,
	),
}

/// Type that implements the `CoretimeInterface` for the allocation of Coretime. Meant to operate
/// from the parachain context. That is, the parachain provides a market (broker) for the sale of
/// coretime, but assumes a `CoretimeProvider` (i.e. a Relay Chain) to actually provide cores.
pub struct CoretimeAllocator;
impl CoretimeInterface for CoretimeAllocator {
	type AccountId = AccountId;
	type Balance = Balance;
	type RelayChainBlockNumberProvider = RelaychainDataProvider<Runtime>;

	fn request_core_count(count: CoreIndex) {
		use CoretimeProviderCalls::RequestCoreCount;
		let request_core_count_call = RelayRuntimePallets::Coretime(RequestCoreCount(count));

		// Weight for `request_core_count` from westend benchmarks:
		// `ref_time` = 7889000 + (3 * 25000000) + (1 * 100000000) = 182889000
		// `proof_size` = 1636
		// Add 5% to each component and round to 2 significant figures.
		let call_weight = Weight::from_parts(190_000_000, 1700);

		let message = Xcm(vec![
			Instruction::UnpaidExecution {
				weight_limit: WeightLimit::Unlimited,
				check_origin: None,
			},
			Instruction::Transact {
				origin_kind: OriginKind::Native,
				call: request_core_count_call.encode().into(),
				fallback_max_weight: Some(call_weight),
			},
		]);

		match PolkadotXcm::send_xcm(Here, Location::parent(), message.clone()) {
			Ok(_) => tracing::debug!(
				target: "runtime::coretime",
				"Request to update schedulable cores sent successfully."
			),
			Err(e) => tracing::error!(
				target: "runtime::coretime", error=?e,
				"Failed to send request to update schedulable cores"
			),
		}
	}

	fn request_revenue_info_at(when: RCBlockNumberOf<Self>) {
		use CoretimeProviderCalls::RequestRevenueInfoAt;
		let request_revenue_info_at_call =
			RelayRuntimePallets::Coretime(RequestRevenueInfoAt(when));

		let message = Xcm(vec![
			Instruction::UnpaidExecution {
				weight_limit: WeightLimit::Unlimited,
				check_origin: None,
			},
			Instruction::Transact {
				origin_kind: OriginKind::Native,
				call: request_revenue_info_at_call.encode().into(),
				fallback_max_weight: Some(Weight::from_parts(1_000_000_000, 200_000)),
			},
		]);

		match PolkadotXcm::send_xcm(Here, Location::parent(), message.clone()) {
			Ok(_) => tracing::debug!(
				target: "runtime::coretime",
				"Request for revenue information sent successfully."
			),
			Err(e) => tracing::error!(
				target: "runtime::coretime", error=?e,
				"Request for revenue information failed to send"
			),
		}
	}

	fn credit_account(who: Self::AccountId, amount: Self::Balance) {
		use CoretimeProviderCalls::CreditAccount;
		let credit_account_call = RelayRuntimePallets::Coretime(CreditAccount(who, amount));

		let message = Xcm(vec![
			Instruction::UnpaidExecution {
				weight_limit: WeightLimit::Unlimited,
				check_origin: None,
			},
			Instruction::Transact {
				origin_kind: OriginKind::Native,
				call: credit_account_call.encode().into(),
				fallback_max_weight: Some(Weight::from_parts(1_000_000_000, 200_000)),
			},
		]);

		match PolkadotXcm::send_xcm(Here, Location::parent(), message.clone()) {
			Ok(_) => tracing::debug!(
				target: "runtime::coretime",
				"Instruction to credit account sent successfully."
			),
			Err(e) => tracing::error!(
				target: "runtime::coretime", error=?e,
				"Instruction to credit account failed to send"
			),
		}
	}

	fn assign_core(
		core: CoreIndex,
		begin: RCBlockNumberOf<Self>,
		assignment: Vec<(CoreAssignment, PartsOf57600)>,
		end_hint: Option<RCBlockNumberOf<Self>>,
	) {
		use CoretimeProviderCalls::AssignCore;

		// Weight for `assign_core` from westend benchmarks:
		// `ref_time` = 10177115 + (1 * 25000000) + (2 * 100000000) + (57600 * 13932) = 937660315
		// `proof_size` = 3612
		// Add 5% to each component and round to 2 significant figures.
		let call_weight = Weight::from_parts(980_000_000, 3800);

		// A maximum of 28 assignments fit in one message, so we split the assignments and send as
		// multiple messages. This will get reassembled into a full list of assignments on the
		// relay chain side.

		for chunk in assignment.chunks(28) {
			let partial_assignment = chunk.to_vec();

			let assign_core_call = RelayRuntimePallets::Coretime(AssignCore(
				core,
				begin,
				partial_assignment,
				end_hint,
			));

			let message = Xcm(vec![
				Instruction::UnpaidExecution {
					weight_limit: WeightLimit::Unlimited,
					check_origin: None,
				},
				Instruction::Transact {
					origin_kind: OriginKind::Native,
					call: assign_core_call.encode().into(),
					fallback_max_weight: Some(call_weight),
				},
			]);

			match PolkadotXcm::send_xcm(Here, Location::parent(), message) {
				Ok(_) => tracing::debug!(
					target: "runtime::coretime",
					"Core assignment sent successfully."
				),
				Err(e) => tracing::error!(
					target: "runtime::coretime", error=?e,
					"Core assignment failed to send"
				),
			}
		}
	}
}
