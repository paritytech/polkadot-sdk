// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use crate::{mock::*, *};
use frame_support::{assert_noop, assert_ok};
use hex_literal::hex;
use snowbridge_core::eth;
use sp_core::H256;
use sp_runtime::{AccountId32, DispatchError::BadOrigin, TokenError};

#[test]
fn create_agent() {
	new_test_ext(true).execute_with(|| {
		let origin_para_id = 2000;
		let origin_location = Location::new(1, [Parachain(origin_para_id)]);
		let agent_id = make_agent_id(origin_location.clone());
		let sovereign_account = sibling_sovereign_account::<Test>(origin_para_id.into());

		// fund sovereign account of origin
		let _ = Balances::mint_into(&sovereign_account, 10000);

		assert!(!Agents::<Test>::contains_key(agent_id));

		let origin = make_xcm_origin(origin_location);
		assert_ok!(EthereumSystem::create_agent(origin));

		assert!(Agents::<Test>::contains_key(agent_id));
	});
}

#[test]
fn test_agent_for_here() {
	new_test_ext(true).execute_with(|| {
		let origin_location = Location::here();
		let agent_id = make_agent_id(origin_location);
		assert_eq!(
			agent_id,
			hex!("03170a2e7597b7b7e3d84c05391d139a62b157e78786d8c082f29dcf4c111314").into(),
		)
	});
}

#[test]
fn create_agent_fails_on_funds_unavailable() {
	new_test_ext(true).execute_with(|| {
		let origin_location = Location::new(1, [Parachain(2000)]);
		let origin = make_xcm_origin(origin_location);
		// Reset balance of sovereign_account to zero so to trigger the FundsUnavailable error
		let sovereign_account = sibling_sovereign_account::<Test>(2000.into());
		Balances::set_balance(&sovereign_account, 0);
		assert_noop!(EthereumSystem::create_agent(origin), TokenError::FundsUnavailable);
	});
}

#[test]
fn create_agent_bad_origin() {
	new_test_ext(true).execute_with(|| {
		// relay chain location not allowed
		assert_noop!(
			EthereumSystem::create_agent(make_xcm_origin(Location::new(1, [],))),
			BadOrigin,
		);

		// local account location not allowed
		assert_noop!(
			EthereumSystem::create_agent(make_xcm_origin(Location::new(
				0,
				[Junction::AccountId32 { network: None, id: [67u8; 32] }],
			))),
			BadOrigin,
		);

		// Signed origin not allowed
		assert_noop!(
			EthereumSystem::create_agent(RuntimeOrigin::signed([14; 32].into())),
			BadOrigin
		);

		// None origin not allowed
		assert_noop!(EthereumSystem::create_agent(RuntimeOrigin::none()), BadOrigin);
	});
}

#[test]
fn upgrade_as_root() {
	new_test_ext(true).execute_with(|| {
		let origin = RuntimeOrigin::root();
		let address: H160 = [1_u8; 20].into();
		let code_hash: H256 = [1_u8; 32].into();

		assert_ok!(EthereumSystem::upgrade(origin, address, code_hash, None));

		System::assert_last_event(RuntimeEvent::EthereumSystem(crate::Event::Upgrade {
			impl_address: address,
			impl_code_hash: code_hash,
			initializer_params_hash: None,
		}));
	});
}

#[test]
fn upgrade_as_signed_fails() {
	new_test_ext(true).execute_with(|| {
		let origin = RuntimeOrigin::signed(AccountId32::new([0; 32]));
		let address: H160 = Default::default();
		let code_hash: H256 = Default::default();

		assert_noop!(EthereumSystem::upgrade(origin, address, code_hash, None), BadOrigin);
	});
}

#[test]
fn upgrade_with_params() {
	new_test_ext(true).execute_with(|| {
		let origin = RuntimeOrigin::root();
		let address: H160 = [1_u8; 20].into();
		let code_hash: H256 = [1_u8; 32].into();
		let initializer: Option<Initializer> =
			Some(Initializer { params: [0; 256].into(), maximum_required_gas: 10000 });
		assert_ok!(EthereumSystem::upgrade(origin, address, code_hash, initializer));
	});
}

#[test]
fn set_operating_mode() {
	new_test_ext(true).execute_with(|| {
		let origin = RuntimeOrigin::root();
		let mode = OperatingMode::RejectingOutboundMessages;

		assert_ok!(EthereumSystem::set_operating_mode(origin, mode));

		System::assert_last_event(RuntimeEvent::EthereumSystem(crate::Event::SetOperatingMode {
			mode,
		}));
	});
}

#[test]
fn set_operating_mode_as_signed_fails() {
	new_test_ext(true).execute_with(|| {
		let origin = RuntimeOrigin::signed([14; 32].into());
		let mode = OperatingMode::RejectingOutboundMessages;

		assert_noop!(EthereumSystem::set_operating_mode(origin, mode), BadOrigin);
	});
}

#[test]
fn set_pricing_parameters() {
	new_test_ext(true).execute_with(|| {
		let origin = RuntimeOrigin::root();
		let mut params = Parameters::get();
		params.rewards.local = 7;

		assert_ok!(EthereumSystem::set_pricing_parameters(origin, params));

		assert_eq!(PricingParameters::<Test>::get().rewards.local, 7);
	});
}

#[test]
fn set_pricing_parameters_as_signed_fails() {
	new_test_ext(true).execute_with(|| {
		let origin = RuntimeOrigin::signed([14; 32].into());
		let params = Parameters::get();

		assert_noop!(EthereumSystem::set_pricing_parameters(origin, params), BadOrigin);
	});
}

#[test]
fn set_pricing_parameters_invalid() {
	new_test_ext(true).execute_with(|| {
		let origin = RuntimeOrigin::root();
		let mut params = Parameters::get();
		params.rewards.local = 0;

		assert_noop!(
			EthereumSystem::set_pricing_parameters(origin.clone(), params),
			Error::<Test>::InvalidPricingParameters
		);

		let mut params = Parameters::get();
		params.exchange_rate = 0u128.into();
		assert_noop!(
			EthereumSystem::set_pricing_parameters(origin.clone(), params),
			Error::<Test>::InvalidPricingParameters
		);
		params = Parameters::get();
		params.fee_per_gas = sp_core::U256::zero();
		assert_noop!(
			EthereumSystem::set_pricing_parameters(origin.clone(), params),
			Error::<Test>::InvalidPricingParameters
		);
		params = Parameters::get();
		params.rewards.local = 0;
		assert_noop!(
			EthereumSystem::set_pricing_parameters(origin.clone(), params),
			Error::<Test>::InvalidPricingParameters
		);
		params = Parameters::get();
		params.rewards.remote = sp_core::U256::zero();
		assert_noop!(
			EthereumSystem::set_pricing_parameters(origin, params),
			Error::<Test>::InvalidPricingParameters
		);
	});
}

#[test]
fn set_token_transfer_fees() {
	new_test_ext(true).execute_with(|| {
		let origin = RuntimeOrigin::root();

		assert_ok!(EthereumSystem::set_token_transfer_fees(origin, 1, 1, eth(1)));
	});
}

#[test]
fn set_token_transfer_fees_root_only() {
	new_test_ext(true).execute_with(|| {
		let origin = RuntimeOrigin::signed([14; 32].into());

		assert_noop!(EthereumSystem::set_token_transfer_fees(origin, 1, 1, 1.into()), BadOrigin);
	});
}

#[test]
fn set_token_transfer_fees_invalid() {
	new_test_ext(true).execute_with(|| {
		let origin = RuntimeOrigin::root();

		assert_noop!(
			EthereumSystem::set_token_transfer_fees(origin, 0, 0, 0.into()),
			Error::<Test>::InvalidTokenTransferFees
		);
	});
}

#[test]
fn create_channel() {
	new_test_ext(true).execute_with(|| {
		let origin_para_id = 2000;
		let origin_location = Location::new(1, [Parachain(origin_para_id)]);
		let sovereign_account = sibling_sovereign_account::<Test>(origin_para_id.into());
		let origin = make_xcm_origin(origin_location);

		// fund sovereign account of origin
		let _ = Balances::mint_into(&sovereign_account, 10000);

		assert_ok!(EthereumSystem::create_agent(origin.clone()));
		assert_ok!(EthereumSystem::create_channel(origin, OperatingMode::Normal));
	});
}

#[test]
fn create_channel_fail_already_exists() {
	new_test_ext(true).execute_with(|| {
		let origin_para_id = 2000;
		let origin_location = Location::new(1, [Parachain(origin_para_id)]);
		let sovereign_account = sibling_sovereign_account::<Test>(origin_para_id.into());
		let origin = make_xcm_origin(origin_location);

		// fund sovereign account of origin
		let _ = Balances::mint_into(&sovereign_account, 10000);

		assert_ok!(EthereumSystem::create_agent(origin.clone()));
		assert_ok!(EthereumSystem::create_channel(origin.clone(), OperatingMode::Normal));

		assert_noop!(
			EthereumSystem::create_channel(origin, OperatingMode::Normal),
			Error::<Test>::ChannelAlreadyCreated
		);
	});
}

#[test]
fn create_channel_bad_origin() {
	new_test_ext(true).execute_with(|| {
		// relay chain location not allowed
		assert_noop!(
			EthereumSystem::create_channel(
				make_xcm_origin(Location::new(1, [])),
				OperatingMode::Normal,
			),
			BadOrigin,
		);

		// child of sibling location not allowed
		assert_noop!(
			EthereumSystem::create_channel(
				make_xcm_origin(Location::new(
					1,
					[Parachain(2000), Junction::AccountId32 { network: None, id: [67u8; 32] }],
				)),
				OperatingMode::Normal,
			),
			BadOrigin,
		);

		// local account location not allowed
		assert_noop!(
			EthereumSystem::create_channel(
				make_xcm_origin(Location::new(
					0,
					[Junction::AccountId32 { network: None, id: [67u8; 32] }],
				)),
				OperatingMode::Normal,
			),
			BadOrigin,
		);

		// Signed origin not allowed
		assert_noop!(
			EthereumSystem::create_channel(
				RuntimeOrigin::signed([14; 32].into()),
				OperatingMode::Normal,
			),
			BadOrigin
		);

		// None origin not allowed
		assert_noop!(EthereumSystem::create_agent(RuntimeOrigin::none()), BadOrigin);
	});
}

#[test]
fn update_channel() {
	new_test_ext(true).execute_with(|| {
		let origin_para_id = 2000;
		let origin_location = Location::new(1, [Parachain(origin_para_id)]);
		let sovereign_account = sibling_sovereign_account::<Test>(origin_para_id.into());
		let origin = make_xcm_origin(origin_location);

		// First create the channel
		let _ = Balances::mint_into(&sovereign_account, 10000);
		assert_ok!(EthereumSystem::create_agent(origin.clone()));
		assert_ok!(EthereumSystem::create_channel(origin.clone(), OperatingMode::Normal));

		// Now try to update it
		assert_ok!(EthereumSystem::update_channel(origin, OperatingMode::Normal));

		System::assert_last_event(RuntimeEvent::EthereumSystem(crate::Event::UpdateChannel {
			channel_id: ParaId::from(2000).into(),
			mode: OperatingMode::Normal,
		}));
	});
}

#[test]
fn update_channel_bad_origin() {
	new_test_ext(true).execute_with(|| {
		let mode = OperatingMode::Normal;

		// relay chain location not allowed
		assert_noop!(
			EthereumSystem::update_channel(make_xcm_origin(Location::new(1, [])), mode,),
			BadOrigin,
		);

		// child of sibling location not allowed
		assert_noop!(
			EthereumSystem::update_channel(
				make_xcm_origin(Location::new(
					1,
					[Parachain(2000), Junction::AccountId32 { network: None, id: [67u8; 32] }],
				)),
				mode,
			),
			BadOrigin,
		);

		// local account location not allowed
		assert_noop!(
			EthereumSystem::update_channel(
				make_xcm_origin(Location::new(
					0,
					[Junction::AccountId32 { network: None, id: [67u8; 32] }],
				)),
				mode,
			),
			BadOrigin,
		);

		// Signed origin not allowed
		assert_noop!(
			EthereumSystem::update_channel(RuntimeOrigin::signed([14; 32].into()), mode),
			BadOrigin
		);

		// None origin not allowed
		assert_noop!(EthereumSystem::update_channel(RuntimeOrigin::none(), mode), BadOrigin);
	});
}

#[test]
fn update_channel_fails_not_exist() {
	new_test_ext(true).execute_with(|| {
		let origin_location = Location::new(1, [Parachain(2000)]);
		let origin = make_xcm_origin(origin_location);

		// Now try to update it
		assert_noop!(
			EthereumSystem::update_channel(origin, OperatingMode::Normal),
			Error::<Test>::NoChannel
		);
	});
}

#[test]
fn force_update_channel() {
	new_test_ext(true).execute_with(|| {
		let origin_para_id = 2000;
		let origin_location = Location::new(1, [Parachain(origin_para_id)]);
		let sovereign_account = sibling_sovereign_account::<Test>(origin_para_id.into());
		let origin = make_xcm_origin(origin_location);

		let channel_id: ChannelId = ParaId::from(origin_para_id).into();

		// First create the channel
		let _ = Balances::mint_into(&sovereign_account, 10000);
		assert_ok!(EthereumSystem::create_agent(origin.clone()));
		assert_ok!(EthereumSystem::create_channel(origin.clone(), OperatingMode::Normal));

		// Now try to force update it
		let force_origin = RuntimeOrigin::root();
		assert_ok!(EthereumSystem::force_update_channel(
			force_origin,
			channel_id,
			OperatingMode::Normal,
		));

		System::assert_last_event(RuntimeEvent::EthereumSystem(crate::Event::UpdateChannel {
			channel_id: ParaId::from(2000).into(),
			mode: OperatingMode::Normal,
		}));
	});
}

#[test]
fn force_update_channel_bad_origin() {
	new_test_ext(true).execute_with(|| {
		let mode = OperatingMode::Normal;

		// signed origin not allowed
		assert_noop!(
			EthereumSystem::force_update_channel(
				RuntimeOrigin::signed([14; 32].into()),
				ParaId::from(1000).into(),
				mode,
			),
			BadOrigin,
		);
	});
}

#[test]
fn transfer_native_from_agent() {
	new_test_ext(true).execute_with(|| {
		let origin_location = Location::new(1, [Parachain(2000)]);
		let origin = make_xcm_origin(origin_location.clone());
		let recipient: H160 = [27u8; 20].into();
		let amount = 103435;

		// First create the agent and channel
		assert_ok!(EthereumSystem::create_agent(origin.clone()));
		assert_ok!(EthereumSystem::create_channel(origin, OperatingMode::Normal));

		let origin = make_xcm_origin(origin_location.clone());
		assert_ok!(EthereumSystem::transfer_native_from_agent(origin, recipient, amount),);

		System::assert_last_event(RuntimeEvent::EthereumSystem(
			crate::Event::TransferNativeFromAgent {
				agent_id: make_agent_id(origin_location),
				recipient,
				amount,
			},
		));
	});
}

#[test]
fn force_transfer_native_from_agent() {
	new_test_ext(true).execute_with(|| {
		let origin = RuntimeOrigin::root();
		let location = Location::new(1, [Parachain(2000)]);
		let versioned_location: Box<VersionedLocation> = Box::new(location.clone().into());
		let recipient: H160 = [27u8; 20].into();
		let amount = 103435;

		// First create the agent
		Agents::<Test>::insert(make_agent_id(location.clone()), ());

		assert_ok!(EthereumSystem::force_transfer_native_from_agent(
			origin,
			versioned_location,
			recipient,
			amount
		),);

		System::assert_last_event(RuntimeEvent::EthereumSystem(
			crate::Event::TransferNativeFromAgent {
				agent_id: make_agent_id(location),
				recipient,
				amount,
			},
		));
	});
}

#[test]
fn force_transfer_native_from_agent_bad_origin() {
	new_test_ext(true).execute_with(|| {
		let recipient: H160 = [27u8; 20].into();
		let amount = 103435;

		// signed origin not allowed
		assert_noop!(
			EthereumSystem::force_transfer_native_from_agent(
				RuntimeOrigin::signed([14; 32].into()),
				Box::new(
					Location::new(
						1,
						[Parachain(2000), Junction::AccountId32 { network: None, id: [67u8; 32] }],
					)
					.into()
				),
				recipient,
				amount,
			),
			BadOrigin,
		);
	});
}

// NOTE: The following tests are not actually tests and are more about obtaining location
// conversions for devops purposes. They need to be removed here and incorporated into a command
// line utility.

#[test]
fn charge_fee_for_create_agent() {
	new_test_ext(true).execute_with(|| {
		let para_id: u32 = TestParaId::get();
		let origin_location = Location::new(1, [Parachain(para_id)]);
		let origin = make_xcm_origin(origin_location.clone());
		let sovereign_account = sibling_sovereign_account::<Test>(para_id.into());
		let (_, agent_id) = ensure_sibling::<Test>(&origin_location).unwrap();

		let initial_sovereign_balance = Balances::balance(&sovereign_account);
		assert_ok!(EthereumSystem::create_agent(origin.clone()));
		let fee_charged = initial_sovereign_balance - Balances::balance(&sovereign_account);

		assert_ok!(EthereumSystem::create_channel(origin, OperatingMode::Normal));

		// assert sovereign_balance decreased by (fee.base_fee + fee.delivery_fee)
		let message = Message {
			id: None,
			channel_id: ParaId::from(para_id).into(),
			command: Command::CreateAgent { agent_id },
		};
		let (_, fee) = OutboundQueue::validate(&message).unwrap();
		assert_eq!(fee.local + fee.remote, fee_charged);

		// and treasury_balance increased
		let treasury_balance = Balances::balance(&TreasuryAccount::get());
		assert!(treasury_balance > InitialFunding::get());

		let final_sovereign_balance = Balances::balance(&sovereign_account);
		// (sovereign_balance + treasury_balance) keeps the same
		assert_eq!(final_sovereign_balance + treasury_balance, { InitialFunding::get() * 2 });
	});
}

#[test]
fn charge_fee_for_transfer_native_from_agent() {
	new_test_ext(true).execute_with(|| {
		let para_id: u32 = TestParaId::get();
		let origin_location = Location::new(1, [Parachain(para_id)]);
		let recipient: H160 = [27u8; 20].into();
		let amount = 103435;
		let origin = make_xcm_origin(origin_location.clone());
		let (_, agent_id) = ensure_sibling::<Test>(&origin_location).unwrap();

		let sovereign_account = sibling_sovereign_account::<Test>(para_id.into());

		// create_agent & create_channel first
		assert_ok!(EthereumSystem::create_agent(origin.clone()));
		assert_ok!(EthereumSystem::create_channel(origin.clone(), OperatingMode::Normal));

		// assert sovereign_balance decreased by only the base_fee
		let sovereign_balance_before = Balances::balance(&sovereign_account);
		assert_ok!(EthereumSystem::transfer_native_from_agent(origin.clone(), recipient, amount));
		let message = Message {
			id: None,
			channel_id: ParaId::from(para_id).into(),
			command: Command::TransferNativeFromAgent { agent_id, recipient, amount },
		};
		let (_, fee) = OutboundQueue::validate(&message).unwrap();
		let sovereign_balance_after = Balances::balance(&sovereign_account);
		assert_eq!(sovereign_balance_after + fee.local, sovereign_balance_before);
	});
}

#[test]
fn charge_fee_for_upgrade() {
	new_test_ext(true).execute_with(|| {
		let para_id: u32 = TestParaId::get();
		let origin = RuntimeOrigin::root();
		let address: H160 = [1_u8; 20].into();
		let code_hash: H256 = [1_u8; 32].into();
		let initializer: Option<Initializer> =
			Some(Initializer { params: [0; 256].into(), maximum_required_gas: 10000 });
		assert_ok!(EthereumSystem::upgrade(origin, address, code_hash, initializer.clone()));

		// assert sovereign_balance does not change as we do not charge for sudo operations
		let sovereign_account = sibling_sovereign_account::<Test>(para_id.into());
		let sovereign_balance = Balances::balance(&sovereign_account);
		assert_eq!(sovereign_balance, InitialFunding::get());
	});
}

#[test]
fn genesis_build_initializes_correctly() {
	new_test_ext(true).execute_with(|| {
		assert!(EthereumSystem::is_initialized(), "Ethereum uninitialized.");
	});
}

#[test]
fn no_genesis_build_is_uninitialized() {
	new_test_ext(false).execute_with(|| {
		assert!(!EthereumSystem::is_initialized(), "Ethereum initialized.");
	});
}
