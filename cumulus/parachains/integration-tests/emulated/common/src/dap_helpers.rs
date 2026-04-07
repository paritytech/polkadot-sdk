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

use frame_support::{
	assert_ok,
	traits::{
		Get, Hooks,
		fungible::{Inspect as FungibleInspect, Mutate as FungibleMutate},
	},
	weights::Weight,
};
use parachains_common::{AccountId, Balance};
use sp_dap::DAP_BUFFER_PALLET_ID;
use sp_runtime::traits::AccountIdConversion;
use xcm_emulator::{Chain, TestExt};

/// Tests that the DAP satellite accumulates native tokens and teleports them to the DAP buffer
/// account on AssetHub when `on_idle` is triggered.
pub fn test_dap_satellite_transfers_to_asset_hub<Sender, AH>(fund_sender: fn(AccountId, Balance))
where
	Sender: Chain + TestExt,
	Sender::Runtime: pallet_dap_satellite::Config
		+ pallet_balances::Config<Balance = Balance>
		+ frame_system::Config<AccountId = AccountId>,
	Sender::RuntimeEvent: TryInto<pallet_dap_satellite::Event<Sender::Runtime>>,
	pallet_dap_satellite::Pallet<Sender::Runtime>: Hooks<u32>,
	<Sender::Runtime as pallet_dap_satellite::Config>::MinTransferAmount: Get<Balance>,
	AH: Chain + TestExt,
	AH::Runtime: pallet_xcm::Config
		+ pallet_balances::Config<Balance = Balance>
		+ pallet_message_queue::Config
		+ frame_system::Config<AccountId = AccountId>,
	AH::RuntimeEvent: TryInto<pallet_message_queue::Event<AH::Runtime>>,
{
	let sender_ed = <Sender::Runtime as pallet_balances::Config>::ExistentialDeposit::get();
	let ah_ed = <AH::Runtime as pallet_balances::Config>::ExistentialDeposit::get();
	let satellite_account = pallet_dap_satellite::Pallet::<Sender::Runtime>::satellite_account();
	let dap_buffer_account: AccountId = DAP_BUFFER_PALLET_ID.into_account_truncating();

	// The fund amount should slightly exceed MinTransferAmount to trigger a transfer.
	let fund_amount =
		<Sender::Runtime as pallet_dap_satellite::Config>::MinTransferAmount::get() + 1;
	fund_sender(satellite_account.clone(), sender_ed + fund_amount);

	// Pre-fund AH's CheckingAccount, as during testing the sender mints its own tokens rather
	// than receiving them from AH via teleport (which would normally accrue them).
	let check_account: AccountId =
		AH::execute_with(|| pallet_xcm::Pallet::<AH::Runtime>::check_account());
	AH::execute_with(|| {
		assert_ok!(pallet_balances::Pallet::<AH::Runtime>::mint_into(
			&check_account,
			fund_amount + ah_ed
		));
	});

	let satellite_balance_before = Sender::account_data_of(satellite_account.clone()).free;
	let available_funds = satellite_balance_before - sender_ed;

	let sender_total_issuance_before =
		Sender::execute_with(|| pallet_balances::Pallet::<Sender::Runtime>::total_issuance());

	let (ah_total_issuance_before, ah_inactive_issuance_before, buffer_balance_before) =
		AH::execute_with(|| {
			(
				pallet_balances::Pallet::<AH::Runtime>::total_issuance(),
				pallet_balances::Pallet::<AH::Runtime>::inactive_issuance(),
				pallet_balances::Pallet::<AH::Runtime>::balance(&dap_buffer_account),
			)
		});

	// The transfer period is 10 blocks (1 Westend minute at 6s block time).
	let transfer_period: u32 = 10;

	// Trigger `on_idle` to initiate a transfer to DAP.
	Sender::execute_with(|| {
		let _ = <pallet_dap_satellite::Pallet<Sender::Runtime> as Hooks<u32>>::on_idle(
			transfer_period + 1,
			Weight::MAX,
		);
		let send_succeeded = Sender::events()
			.into_iter()
			.any(|e| matches!(e.try_into(), Ok(pallet_dap_satellite::Event::SendSucceeded { .. })));
		assert!(send_succeeded, "Expected DapSatellite::SendSucceeded event");
	});

	// Delivery fees are waived for the satellite, so it retains exactly the ED.
	let satellite_balance_after = Sender::account_data_of(satellite_account).free;
	assert_eq!(satellite_balance_after, sender_ed);

	let sender_total_issuance_after =
		Sender::execute_with(|| pallet_balances::Pallet::<Sender::Runtime>::total_issuance());
	assert_eq!(sender_total_issuance_after, sender_total_issuance_before - available_funds);

	// The XCM message is delivered to AH on the first execute_with call.
	AH::execute_with(|| {
		let mq_processed = AH::events().into_iter().any(|e| {
			matches!(e.try_into(), Ok(pallet_message_queue::Event::Processed { success: true, .. }))
		});
		assert!(mq_processed, "Expected MessageQueue::Processed(success: true) on AssetHub");

		let buffer_balance_after =
			pallet_balances::Pallet::<AH::Runtime>::balance(&dap_buffer_account);
		assert!(buffer_balance_after > buffer_balance_before);
		let amount_received = buffer_balance_after - buffer_balance_before;

		// Ensure the total issuance is unchanged (teleport is burn-on-send / mint-on-receive).
		assert_eq!(
			pallet_balances::Pallet::<AH::Runtime>::total_issuance(),
			ah_total_issuance_before
		);

		// Ensure the inactive issuance has increased.
		assert_eq!(
			pallet_balances::Pallet::<AH::Runtime>::inactive_issuance(),
			ah_inactive_issuance_before + amount_received
		);
	});
}
