// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

use crate::{xcm_config::LocationToAccountId, *};
use codec::{Decode, Encode};
use cumulus_pallet_parachain_system::RelaychainDataProvider;
use cumulus_primitives_core::relay_chain;
use frame_support::parameter_types;
use pallet_broker::{
	CoreAssignment, CoreIndex, CoretimeInterface, PartsOf57600, RCBlockNumberOf, TaskId, Timeslice,
};
use parachains_common::{AccountId, Balance};
use sp_runtime::traits::MaybeConvert;
use westend_runtime_constants::system_parachain::coretime;
use xcm::latest::prelude::*;
use xcm_executor::traits::ConvertLocation;

/// Coretime revenue handler that sends funds to the DAP satellite account.
///
/// Previously, revenue was accumulated in a stash account and then burned at the relay chain.
/// With DAP, revenue is accumulated in the satellite account and then periodically sent
/// to the DAP buffer on AssetHub via XCM.
pub type CoretimeRevenueToSatellite = pallet_dap_satellite::SlashToSatellite<Runtime>;

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

parameter_types! {
	pub const BrokerPalletId: PalletId = PalletId(*b"py/broke");
	pub const MinimumCreditPurchase: Balance = UNITS / 10;
	pub const MinimumEndPrice: Balance = UNITS;
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
		use crate::coretime::CoretimeProviderCalls::AssignCore;

		// Weight for `assign_core` from westend benchmarks:
		// `ref_time` = 10177115 + (1 * 25000000) + (2 * 100000000) + (57600 * 13932) = 937660315
		// `proof_size` = 3612
		// Add 5% to each component and round to 2 significant figures.
		let call_weight = Weight::from_parts(980_000_000, 3800);

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
				fallback_max_weight: Some(call_weight),
			},
		]);

		match PolkadotXcm::send_xcm(Here, Location::parent(), message.clone()) {
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

	fn on_new_timeslice(_timeslice: Timeslice) {
		// With DAP satellite, revenue is already accumulated in the satellite account via
		// CoretimeRevenueToSatellite (OnRevenue handler). The satellite pallet then sends
		// accumulated funds to AssetHub DAP via XCM
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
	type OnRevenue = CoretimeRevenueToSatellite;
	type TimeslicePeriod = ConstU32<{ coretime::TIMESLICE_PERIOD }>;
	// We don't actually need any leases at launch but set to 10 in case we want to sudo some in.
	type MaxLeasedCores = ConstU32<10>;
	type MaxReservedCores = ConstU32<10>;
	type Coretime = CoretimeAllocator;
	type ConvertBalance = sp_runtime::traits::Identity;
	type WeightInfo = weights::pallet_broker::WeightInfo<Runtime>;
	type PalletId = BrokerPalletId;
	type AdminOrigin = EnsureRoot<AccountId>;
	type SovereignAccountOf = SovereignAccountOf;
	type MaxAutoRenewals = ConstU32<20>;
	type PriceAdapter = pallet_broker::MinimumPrice<Balance, MinimumEndPrice>;
	type MinimumCreditPurchase = MinimumCreditPurchase;
}
