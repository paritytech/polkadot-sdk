// Copyright Parity Technologies (UK) Ltd.
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

use crate::*;

const MAX_CAPACITY: u32 = 8;
const MAX_MESSAGE_SIZE: u32 = 8192;

/// Opening HRMP channels between Parachains should work
#[test]
#[cfg(feature = "FIXME-IGNORED")] // <https://github.com/paritytech/cumulus/issues/3027>
fn open_hrmp_channel_between_paras_works() {
	// Parchain A init values
	let para_a_id = PenpalKusamaA::para_id();
	let para_a_root_origin = <PenpalKusamaA as Chain>::RuntimeOrigin::root();

	// Parachain B init values
	let para_b_id = PenpalKusamaB::para_id();
	let para_b_root_origin = <PenpalKusamaB as Chain>::RuntimeOrigin::root();

	let fee_amount = KUSAMA_ED * 1000;
	let fund_amount = KUSAMA_ED * 1000_000_000;

	// Fund Parachain's Sovereign accounts to be able to reserve the deposit
	let para_a_sovereign_account = Kusama::fund_para_sovereign(fund_amount, para_a_id);
	let para_b_sovereign_account = Kusama::fund_para_sovereign(fund_amount, para_b_id);

	let relay_destination: VersionedMultiLocation = PenpalKusamaA::parent_location().into();

	// ---- Init Open channel from Parachain to System Parachain
	let mut call = Kusama::init_open_channel_call(para_b_id, MAX_CAPACITY, MAX_MESSAGE_SIZE);
	let origin_kind = OriginKind::Native;
	let native_asset: MultiAsset = (Here, fee_amount).into();
	let beneficiary = Kusama::sovereign_account_id_of_child_para(para_a_id);

	let mut xcm = xcm_transact_paid_execution(call, origin_kind, native_asset.clone(), beneficiary);

	PenpalKusamaA::execute_with(|| {
		assert_ok!(<PenpalKusamaA as PenpalKusamaAPallet>::PolkadotXcm::send(
			para_a_root_origin,
			bx!(relay_destination.clone()),
			bx!(xcm),
		));

		PenpalKusamaA::assert_xcm_pallet_sent();
	});

	Kusama::execute_with(|| {
		type RuntimeEvent = <Kusama as Chain>::RuntimeEvent;

		Kusama::assert_ump_queue_processed(
			true,
			Some(para_a_id),
			Some(Weight::from_parts(1_312_558_000, 200000)),
		);

		assert_expected_events!(
			Kusama,
			vec![
				// Parachain's Sovereign account balance is withdrawn to pay XCM fees
				RuntimeEvent::Balances(pallet_balances::Event::Withdraw { who, amount }) => {
					who: *who == para_a_sovereign_account.clone(),
					amount: *amount == fee_amount,
				},
				// Sender deposit is reserved for Parachain's Sovereign account
				RuntimeEvent::Balances(pallet_balances::Event::Reserved { who, .. }) =>{
					who: *who == para_a_sovereign_account,
				},
				// Open channel requested from Para A to Para B
				RuntimeEvent::Hrmp(
					polkadot_runtime_parachains::hrmp::Event::OpenChannelRequested(
						sender, recipient, max_capacity, max_message_size
					)
				) => {
					sender: *sender == para_a_id.into(),
					recipient: *recipient == para_b_id.into(),
					max_capacity: *max_capacity == MAX_CAPACITY,
					max_message_size: *max_message_size == MAX_MESSAGE_SIZE,
				},
			]
		);
	});

	// ---- Accept Open channel from Parachain to System Parachain
	call = Kusama::accept_open_channel_call(para_a_id);
	let beneficiary = Kusama::sovereign_account_id_of_child_para(para_b_id);

	xcm = xcm_transact_paid_execution(call, origin_kind, native_asset, beneficiary);

	PenpalKusamaB::execute_with(|| {
		assert_ok!(<PenpalKusamaB as PenpalKusamaBPallet>::PolkadotXcm::send(
			para_b_root_origin,
			bx!(relay_destination),
			bx!(xcm),
		));

		PenpalKusamaB::assert_xcm_pallet_sent();
	});

	Kusama::execute_with(|| {
		type RuntimeEvent = <Kusama as Chain>::RuntimeEvent;

		Kusama::assert_ump_queue_processed(
			true,
			Some(para_b_id),
			Some(Weight::from_parts(1_312_558_000, 200_000)),
		);

		assert_expected_events!(
			Kusama,
			vec![
				// Parachain's Sovereign account balance is withdrawn to pay XCM fees
				RuntimeEvent::Balances(pallet_balances::Event::Withdraw { who, amount }) => {
					who: *who == para_b_sovereign_account.clone(),
					amount: *amount == fee_amount,
				},
				// Sender deposit is reserved for Parachain's Sovereign account
				RuntimeEvent::Balances(pallet_balances::Event::Reserved { who, .. }) =>{
					who: *who == para_b_sovereign_account,
				},
				// Open channel accepted for Para A to Para B
				RuntimeEvent::Hrmp(
					polkadot_runtime_parachains::hrmp::Event::OpenChannelAccepted(
						sender, recipient
					)
				) => {
					sender: *sender == para_a_id.into(),
					recipient: *recipient == para_b_id.into(),
				},
			]
		);
	});

	Kusama::force_process_hrmp_open(para_a_id, para_b_id);
}

/// Opening HRMP channels between System Parachains and Parachains should work
#[test]
fn force_open_hrmp_channel_for_system_para_works() {
	// Relay Chain init values
	let relay_root_origin = <Kusama as Chain>::RuntimeOrigin::root();

	// System Para init values
	let system_para_id = AssetHubKusama::para_id();

	// Parachain A init values
	let para_a_id = PenpalKusamaA::para_id();

	let fund_amount = KUSAMA_ED * 1000_000_000;

	// Fund Parachain's Sovereign accounts to be able to reserve the deposit
	let para_a_sovereign_account = Kusama::fund_para_sovereign(fund_amount, para_a_id);
	let system_para_sovereign_account = Kusama::fund_para_sovereign(fund_amount, system_para_id);

	Kusama::execute_with(|| {
		assert_ok!(<Kusama as KusamaPallet>::Hrmp::force_open_hrmp_channel(
			relay_root_origin,
			system_para_id,
			para_a_id,
			MAX_CAPACITY,
			MAX_MESSAGE_SIZE
		));

		type RuntimeEvent = <Kusama as Chain>::RuntimeEvent;

		assert_expected_events!(
			Kusama,
			vec![
				// Sender deposit is reserved for System Parachain's Sovereign account
				RuntimeEvent::Balances(pallet_balances::Event::Reserved { who, .. }) =>{
					who: *who == system_para_sovereign_account,
				},
				// Recipient deposit is reserved for Parachain's Sovereign account
				RuntimeEvent::Balances(pallet_balances::Event::Reserved { who, .. }) =>{
					who: *who == para_a_sovereign_account,
				},
				// HRMP channel forced opened
				RuntimeEvent::Hrmp(
					polkadot_runtime_parachains::hrmp::Event::HrmpChannelForceOpened(
						sender, recipient, max_capacity, max_message_size
					)
				) => {
					sender: *sender == system_para_id.into(),
					recipient: *recipient == para_a_id.into(),
					max_capacity: *max_capacity == MAX_CAPACITY,
					max_message_size: *max_message_size == MAX_MESSAGE_SIZE,
				},
			]
		);
	});

	Kusama::force_process_hrmp_open(system_para_id, para_a_id);
}
