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

use crate::{xcm_config::LocationToAccountId, *};
use codec::{Decode, Encode};
use cumulus_pallet_parachain_system::RelaychainDataProvider;
use cumulus_primitives_core::relay_chain;
use frame_support::{
	parameter_types,
	traits::{
		fungible::{Balanced, Credit, Inspect},
		tokens::{Fortitude, Preservation},
		DefensiveResult, OnUnbalanced,
	},
};
use frame_system::Pallet as System;
use pallet_broker::{
	CoreAssignment, CoreIndex, CoretimeInterface, PartsOf57600, RCBlockNumberOf, TaskId,
};
use parachains_common::{AccountId, Balance};
use rococo_runtime_constants::system_parachain::coretime;
use sp_runtime::traits::{AccountIdConversion, MaybeConvert};
use xcm::latest::prelude::*;
use xcm_executor::traits::{ConvertLocation, TransactAsset};

pub struct BurnCoretimeRevenue;
impl OnUnbalanced<Credit<AccountId, Balances>> for BurnCoretimeRevenue {
	fn on_nonzero_unbalanced(amount: Credit<AccountId, Balances>) {
		let acc = RevenueAccumulationAccount::get();
		if !System::<Runtime>::account_exists(&acc) {
			System::<Runtime>::inc_providers(&acc);
		}
		Balances::resolve(&acc, amount).defensive_ok();
	}
}

type AssetTransactor = <xcm_config::XcmConfig as xcm_executor::Config>::AssetTransactor;

fn burn_at_relay(stash: &AccountId, value: Balance) -> Result<(), XcmError> {
	let dest = Location::parent();
	let stash_location =
		Junction::AccountId32 { network: None, id: stash.clone().into() }.into_location();
	let asset = Asset { id: AssetId(Location::parent()), fun: Fungible(value) };
	let dummy_xcm_context = XcmContext { origin: None, message_id: [0; 32], topic: None };

	let withdrawn = AssetTransactor::withdraw_asset(&asset, &stash_location, None)?;

	AssetTransactor::can_check_out(&dest, &asset, &dummy_xcm_context)?;

	let parent_assets = Into::<Assets>::into(withdrawn)
		.reanchored(&dest, &Here.into())
		.defensive_map_err(|_| XcmError::ReanchorFailed)?;

	PolkadotXcm::send_xcm(
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
	)?;

	AssetTransactor::check_out(&dest, &asset, &dummy_xcm_context);

	Ok(())
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
	pub RevenueAccumulationAccount: AccountId = BrokerPalletId::get().into_sub_account_truncating(b"burnstash");
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

		// The relay chain currently only allows `assign_core` to be called with a complete mask
		// and only ever with increasing `begin`. The assignments must be truncated to avoid
		// dropping that core's assignment completely.

		// This shadowing of `assignment` is temporary and can be removed when the relay can accept
		// multiple messages to assign a single core.
		let assignment = if assignment.len() > 28 {
			let mut total_parts = 0u16;
			// Account for missing parts with a new `Idle` assignment at the start as
			// `assign_core` on the relay assumes this is sorted. We'll add the rest of the
			// assignments and sum the parts in one pass, so this is just initialized to 0.
			let mut assignment_truncated = vec![(CoreAssignment::Idle, 0)];
			// Truncate to first 27 non-idle assignments.
			assignment_truncated.extend(
				assignment
					.into_iter()
					.filter(|(a, _)| *a != CoreAssignment::Idle)
					.take(27)
					.inspect(|(_, parts)| total_parts += *parts)
					.collect::<Vec<_>>(),
			);

			// Set the parts of the `Idle` assignment we injected at the start of the vec above.
			assignment_truncated[0].1 = 57_600u16.saturating_sub(total_parts);
			assignment_truncated
		} else {
			assignment
		};

		let assign_core_call =
			RelayRuntimePallets::Coretime(AssignCore(core, begin, assignment, end_hint));

		let message = Xcm(vec![
			Instruction::UnpaidExecution {
				weight_limit: WeightLimit::Unlimited,
				check_origin: None,
			},
			Instruction::Transact {
				origin_kind: OriginKind::Native,
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

	fn on_new_timeslice(_t: pallet_broker::Timeslice) {
		let stash = RevenueAccumulationAccount::get();
		let value =
			Balances::reducible_balance(&stash, Preservation::Expendable, Fortitude::Polite);

		if value > 0 {
			log::debug!(target: "runtime::coretime", "Going to burn {value} stashed tokens at RC");
			match burn_at_relay(&stash, value) {
				Ok(()) => {
					log::debug!(target: "runtime::coretime", "Succesfully burnt {value} tokens");
				},
				Err(err) => {
					log::error!(target: "runtime::coretime", "burn_at_relay failed: {err:?}");
				},
			}
		}
	}
}

pub struct SovereignAccountOf;
impl MaybeConvert<TaskId, AccountId> for SovereignAccountOf {
	fn maybe_convert(id: TaskId) -> Option<AccountId> {
		// Currently all tasks are parachains.
		let location = Location::new(1, [Parachain(id)]);
		LocationToAccountId::convert_location(&location)
	}
}

impl pallet_broker::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type OnRevenue = BurnCoretimeRevenue;
	type TimeslicePeriod = ConstU32<{ coretime::TIMESLICE_PERIOD }>;
	type MaxLeasedCores = ConstU32<50>;
	type MaxReservedCores = ConstU32<10>;
	type Coretime = CoretimeAllocator;
	type ConvertBalance = sp_runtime::traits::Identity;
	type WeightInfo = weights::pallet_broker::WeightInfo<Runtime>;
	type PalletId = BrokerPalletId;
	type AdminOrigin = EnsureRoot<AccountId>;
	type SovereignAccountOf = SovereignAccountOf;
	type MaxAutoRenewals = ConstU32<100>;
	type PriceAdapter = pallet_broker::CenterTargetPrice<Balance>;
}
