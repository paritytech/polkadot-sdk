// Copyright 2022 Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

use core::marker::PhantomData;

use crate::*;
use codec::{Decode, Encode};
use cumulus_pallet_parachain_system::RelaychainDataProvider;
use cumulus_primitives_core::relay_chain;
use frame_support::{
	parameter_types,
	traits::{
		fungible::{Balanced, Credit},
		Imbalance, OnUnbalanced, TryDrop,
	},
};
use pallet_broker::{
	CoreAssignment, CoreIndex, CoretimeInterface, OnDemandRevenueRecord, PartsOf57600,
	RCBlockNumberOf, RevenueInbox,
};
use parachains_common::{AccountId, Balance};
use rococo_runtime_constants::system_parachain::coretime;
use sp_core::Get;
use sp_runtime::traits::AccountIdConversion;
use xcm::latest::prelude::*;

pub struct BurnAtRelay;
impl OnUnbalanced<Credit<AccountId, Balances>> for BurnAtRelay {
	fn on_nonzero_unbalanced(amount: Credit<AccountId, Balances>) {
		let value = amount.peek();

		// The assets being burnt, from the relay chain perspective
		let parent_assets: Assets =
			vec![Asset { id: AssetId(Location::here()), fun: Fungible(value) }].into();

		let send_res = PolkadotXcm::send_xcm(
			Here,
			Location::parent(),
			Xcm(vec![
				Instruction::UnpaidExecution {
					weight_limit: WeightLimit::Unlimited,
					check_origin: None,
				},
				ReceiveTeleportedAsset(parent_assets.clone()),
				BurnAsset(parent_assets),
			]),
		);

		match send_res {
			Ok(_) => {
				log::debug!(
					target: "runtime::coretime",
					"{value} tokens teleported to the RC for burning",
				);

				// Burn the asset on the parachain. It's on the best effort basis
				let debt = <Balances as Balanced<_>>::rescind(value);
				let _ = amount.offset(debt).try_drop().map_err(|rest| drop(rest));
			},
			Err(_) => {
				log::error!(
					target: "runtime::coretime",
					"Failed to transfer {value} tokens to the relay chain for burning",
				);
			},
		}
	}
}

/// A type containing the encoding of the coretime pallet in the Relay chain runtime. Used to
/// construct any remote calls. The codec index must correspond to the index of `Coretime` in the
/// `construct_runtime` of the Relay chain.
#[derive(Encode, Decode)]
enum RelayRuntimePallets {
	#[codec(index = 74)]
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

parameter_types! {
	pub const BrokerPalletId: PalletId = PalletId(*b"py/broke");
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
		use crate::coretime::CoretimeProviderCalls::RequestCoreCount;
		let request_core_count_call = RelayRuntimePallets::Coretime(RequestCoreCount(count));

		let message = Xcm(vec![
			Instruction::UnpaidExecution {
				weight_limit: WeightLimit::Unlimited,
				check_origin: None,
			},
			Instruction::Transact {
				origin_kind: OriginKind::Native,
				require_weight_at_most: Weight::from_parts(1000000000, 200000),
				call: request_core_count_call.encode().into(),
			},
		]);

		match PolkadotXcm::send_xcm(Here, Location::parent(), message.clone()) {
			Ok(_) => log::info!(
				target: "runtime::coretime",
				"Request to update schedulable cores sent successfully."
			),
			Err(e) => log::error!(
				target: "runtime::coretime",
				"Failed to send request to update schedulable cores: {:?}",
				e
			),
		}
	}

	fn request_revenue_info_at(when: RCBlockNumberOf<Self>) {
		use crate::coretime::CoretimeProviderCalls::RequestRevenueInfoAt;
		let request_revenue_info_at_call =
			RelayRuntimePallets::Coretime(RequestRevenueInfoAt(when));

		let message = Xcm(vec![
			Instruction::UnpaidExecution {
				weight_limit: WeightLimit::Unlimited,
				check_origin: None,
			},
			Instruction::Transact {
				origin_kind: OriginKind::Native,
				require_weight_at_most: Weight::from_parts(1000000000, 200000),
				call: request_revenue_info_at_call.encode().into(),
			},
		]);

		match PolkadotXcm::send_xcm(Here, Location::parent(), message.clone()) {
			Ok(_) => log::info!(
				target: "runtime::coretime",
				"Request for revenue information sent successfully."
			),
			Err(e) => log::error!(
				target: "runtime::coretime",
				"Request for revenue information failed to send: {:?}",
				e
			),
		}
	}

	fn credit_account(who: Self::AccountId, amount: Self::Balance) {
		use crate::coretime::CoretimeProviderCalls::CreditAccount;
		let credit_account_call = RelayRuntimePallets::Coretime(CreditAccount(who, amount));

		let message = Xcm(vec![
			Instruction::UnpaidExecution {
				weight_limit: WeightLimit::Unlimited,
				check_origin: None,
			},
			Instruction::Transact {
				origin_kind: OriginKind::Native,
				require_weight_at_most: Weight::from_parts(1000000000, 200000),
				call: credit_account_call.encode().into(),
			},
		]);

		match PolkadotXcm::send_xcm(Here, Location::parent(), message.clone()) {
			Ok(_) => log::info!(
				target: "runtime::coretime",
				"Instruction to credit account sent successfully."
			),
			Err(e) => log::error!(
				target: "runtime::coretime",
				"Instruction to credit account failed to send: {:?}",
				e
			),
		}
	}

	fn assign_core(
		core: CoreIndex,
		begin: RCBlockNumberOf<Self>,
		assignment: Vec<(CoreAssignment, PartsOf57600)>,
		end_hint: Option<RCBlockNumberOf<Self>>,
	) {
		use crate::coretime::CoretimeProviderCalls::AssignCore;
		let assign_core_call =
			RelayRuntimePallets::Coretime(AssignCore(core, begin, assignment, end_hint));

		let message = Xcm(vec![
			Instruction::UnpaidExecution {
				weight_limit: WeightLimit::Unlimited,
				check_origin: None,
			},
			Instruction::Transact {
				origin_kind: OriginKind::Native,
				require_weight_at_most: Weight::from_parts(1_000_000_000, 200000),
				call: assign_core_call.encode().into(),
			},
		]);

		match PolkadotXcm::send_xcm(Here, Location::parent(), message.clone()) {
			Ok(_) => log::info!(
				target: "runtime::coretime",
				"Core assignment sent successfully."
			),
			Err(e) => log::error!(
				target: "runtime::coretime",
				"Core assignment failed to send: {:?}",
				e
			),
		}
	}

	fn check_notify_revenue_info(
	) -> Option<OnDemandRevenueRecord<RCBlockNumberOf<Self>, Self::Balance>> {
		RevenueInbox::<Runtime>::take()
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_notify_revenue_info(
		info: OnDemandRevenueRecord<RCBlockNumberOf<Self>, Self::Balance>,
	) {
		RevenueInbox::<Runtime>::put(info);
	}
}

impl pallet_broker::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type OnRevenue = BurnAtRelay;
	type TimeslicePeriod = ConstU32<{ coretime::TIMESLICE_PERIOD }>;
	type MaxLeasedCores = ConstU32<50>;
	type MaxReservedCores = ConstU32<10>;
	type Coretime = CoretimeAllocator;
	type ConvertBalance = sp_runtime::traits::Identity;
	type WeightInfo = weights::pallet_broker::WeightInfo<Runtime>;
	type PalletId = BrokerPalletId;
	type AdminOrigin = EnsureRoot<AccountId>;
	type PriceAdapter = pallet_broker::CenterTargetPrice<Balance>;
}
