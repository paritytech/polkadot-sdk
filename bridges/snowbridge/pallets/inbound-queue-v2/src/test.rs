// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use super::*;

use crate::{mock::*, Error};
use codec::Encode;
use frame_support::{assert_noop, assert_ok};
use snowbridge_inbound_queue_primitives::{v2::Payload, EventProof, Proof};
use snowbridge_test_utils::{
	mock_rewards::{RegisteredRewardAmount, RegisteredRewardsCount},
	mock_xcm::{set_charge_fees_override, set_sender_override},
};
use sp_keyring::sr25519::Keyring;
use sp_runtime::DispatchError;
use xcm::prelude::*;

#[test]
fn test_submit_happy_path() {
	new_tester().execute_with(|| {
		let relayer: AccountId = Keyring::Bob.into();

		let origin = RuntimeOrigin::signed(relayer.clone());

		// Submit message
		let event = EventProof {
			event_log: mock_event_log(),
			proof: Proof {
				receipt_proof: Default::default(),
				execution_proof: mock_execution_proof(),
			},
		};

		assert_ok!(InboundQueue::submit(origin.clone(), Box::new(event.clone())));

		let events = frame_system::Pallet::<Test>::events();
		assert!(
			events.iter().any(|event| matches!(
				event.event,
				RuntimeEvent::InboundQueue(Event::MessageReceived { nonce, ..})
					if nonce == 1
			)),
			"no message received event emitted."
		);

		assert_eq!(RegisteredRewardsCount::get(), 1, "Relayer reward should have been registered");
	});
}

#[test]
fn test_submit_with_invalid_gateway() {
	new_tester().execute_with(|| {
		let relayer: AccountId = Keyring::Bob.into();
		let origin = RuntimeOrigin::signed(relayer);

		// Submit message
		let event = EventProof {
			event_log: mock_event_log_invalid_gateway(),
			proof: Proof {
				receipt_proof: Default::default(),
				execution_proof: mock_execution_proof(),
			},
		};
		assert_noop!(
			InboundQueue::submit(origin.clone(), Box::new(event.clone())),
			Error::<Test>::InvalidGateway
		);
	});
}

#[test]
fn test_submit_verification_fails_with_invalid_proof() {
	new_tester().execute_with(|| {
		let relayer: AccountId = Keyring::Bob.into();

		let origin = RuntimeOrigin::signed(relayer.clone());

		// Submit message
		let mut event = EventProof {
			event_log: mock_event_log(),
			proof: Proof {
				receipt_proof: Default::default(),
				execution_proof: mock_execution_proof(),
			},
		};
		// The mock verifier will error once it matches this address.
		event.event_log.address = ERROR_ADDRESS.into();

		assert_noop!(
			InboundQueue::submit(origin.clone(), Box::new(event.clone())),
			Error::<Test>::Verification(VerificationError::InvalidProof)
		);
	});
}

#[test]
fn test_submit_fails_with_malformed_message() {
	new_tester().execute_with(|| {
		let relayer: AccountId = Keyring::Bob.into();

		let origin = RuntimeOrigin::signed(relayer.clone());

		// Submit message
		let event = EventProof {
			event_log: mock_event_log_invalid_message(),
			proof: Proof {
				receipt_proof: Default::default(),
				execution_proof: mock_execution_proof(),
			},
		};

		assert_noop!(
			InboundQueue::submit(origin.clone(), Box::new(event.clone())),
			Error::<Test>::InvalidMessage
		);
	});
}

#[test]
fn test_using_same_nonce_fails() {
	new_tester().execute_with(|| {
		let relayer: AccountId = Keyring::Bob.into();

		let origin = RuntimeOrigin::signed(relayer.clone());

		// Submit message
		let event = EventProof {
			event_log: mock_event_log(),
			proof: Proof {
				receipt_proof: Default::default(),
				execution_proof: mock_execution_proof(),
			},
		};

		assert_ok!(InboundQueue::submit(origin.clone(), Box::new(event.clone())));

		let events = frame_system::Pallet::<Test>::events();
		assert!(
			events.iter().any(|event| matches!(
				event.event,
				RuntimeEvent::InboundQueue(Event::MessageReceived { nonce, ..})
					if nonce == 1
			)),
			"no event emitted."
		);

		assert_noop!(
			InboundQueue::submit(origin.clone(), Box::new(event.clone())),
			Error::<Test>::InvalidNonce
		);
	});
}

#[test]
fn test_set_operating_mode() {
	new_tester().execute_with(|| {
		let relayer: AccountId = Keyring::Bob.into();
		let origin = RuntimeOrigin::signed(relayer);
		let event = EventProof {
			event_log: mock_event_log(),
			proof: Proof {
				receipt_proof: Default::default(),
				execution_proof: mock_execution_proof(),
			},
		};

		assert_ok!(InboundQueue::set_operating_mode(
			RuntimeOrigin::root(),
			snowbridge_core::BasicOperatingMode::Halted
		));

		assert_noop!(InboundQueue::submit(origin, Box::new(event)), Error::<Test>::Halted);
	});
}

#[test]
fn test_set_operating_mode_root_only() {
	new_tester().execute_with(|| {
		assert_noop!(
			InboundQueue::set_operating_mode(
				RuntimeOrigin::signed(Keyring::Bob.into()),
				snowbridge_core::BasicOperatingMode::Halted
			),
			DispatchError::BadOrigin
		);
	});
}

#[test]
fn test_xcm_send_failure() {
	crate::test::new_tester().execute_with(|| {
		set_sender_override(
			|dest: &mut Option<Location>, xcm: &mut Option<Xcm<()>>| {
				if let Some(location) = dest {
					match location.unpack() {
						(_, [Parachain(1001)]) => return Err(SendError::NotApplicable),
						_ => Ok((xcm.clone().unwrap(), Assets::default())),
					}
				} else {
					Ok((xcm.clone().unwrap(), Assets::default()))
				}
			},
			|_| Err(SendError::DestinationUnsupported),
		);
		let relayer: AccountId = Keyring::Bob.into();

		let origin = mock::RuntimeOrigin::signed(relayer.clone());

		// Submit message
		let event = EventProof {
			event_log: mock_event_log(),
			proof: Proof {
				receipt_proof: Default::default(),
				execution_proof: mock_execution_proof(),
			},
		};

		assert_noop!(
			crate::test::InboundQueue::submit(origin.clone(), Box::new(event.clone())),
			Error::<Test>::SendFailure
		);
	});
}

#[test]
fn test_xcm_send_validate_failure() {
	crate::test::new_tester().execute_with(|| {
		set_sender_override(
			|_, _| return Err(SendError::NotApplicable),
			|xcm| {
				let hash = xcm.using_encoded(sp_io::hashing::blake2_256);
				Ok(hash)
			},
		);
		let relayer: AccountId = Keyring::Bob.into();

		let origin = mock::RuntimeOrigin::signed(relayer.clone());

		// Submit message
		let event = EventProof {
			event_log: mock_event_log(),
			proof: Proof {
				receipt_proof: Default::default(),
				execution_proof: mock_execution_proof(),
			},
		};

		assert_noop!(
			crate::test::InboundQueue::submit(origin.clone(), Box::new(event.clone())),
			Error::<Test>::Unreachable
		);
	});
}

#[test]
fn test_xcm_charge_fees_failure() {
	crate::test::new_tester().execute_with(|| {
		set_charge_fees_override(|_, _| Err(XcmError::FeesNotMet));

		let relayer: AccountId = Keyring::Bob.into();

		let origin = mock::RuntimeOrigin::signed(relayer.clone());

		// Submit message
		let event = EventProof {
			event_log: mock_event_log(),
			proof: Proof {
				receipt_proof: Default::default(),
				execution_proof: mock_execution_proof(),
			},
		};

		assert_noop!(
			crate::test::InboundQueue::submit(origin.clone(), Box::new(event.clone())),
			Error::<Test>::FeesNotMet
		);
	});
}

#[test]
fn test_register_token() {
	new_tester().execute_with(|| {
		let relayer: AccountId = Keyring::Bob.into();
		let origin = RuntimeOrigin::signed(relayer);
		let event = EventProof {
			event_log: mock_event_log_v2(),
			proof: Proof {
				receipt_proof: Default::default(),
				execution_proof: mock_execution_proof(),
			},
		};

		assert_ok!(InboundQueue::submit(origin, Box::new(event)));
	});
}

#[test]
fn test_switch_operating_mode() {
	new_tester().execute_with(|| {
		let relayer: AccountId = Keyring::Bob.into();
		let origin = RuntimeOrigin::signed(relayer);
		let event = EventProof {
			event_log: mock_event_log(),
			proof: Proof {
				receipt_proof: Default::default(),
				execution_proof: mock_execution_proof(),
			},
		};

		assert_ok!(InboundQueue::set_operating_mode(
			RuntimeOrigin::root(),
			snowbridge_core::BasicOperatingMode::Halted
		));

		assert_noop!(
			InboundQueue::submit(origin.clone(), Box::new(event.clone())),
			Error::<Test>::Halted
		);

		assert_ok!(InboundQueue::set_operating_mode(
			RuntimeOrigin::root(),
			snowbridge_core::BasicOperatingMode::Normal
		));

		assert_ok!(InboundQueue::submit(origin, Box::new(event)));
	});
}

#[test]
fn zero_reward_does_not_register_reward() {
	new_tester().execute_with(|| {
		let relayer: AccountId = Keyring::Bob.into();
		let origin = H160::random();
		assert_ok!(InboundQueue::process_message(
			relayer,
			Message {
				nonce: 0,
				assets: vec![],
				payload: Payload::Raw(vec![]),
				claimer: None,
				execution_fee: 1_000_000_000,
				relayer_fee: 0,
				gateway: GatewayAddress::get(),
				origin,
				value: 3_000_000_000,
			}
		));

		assert_eq!(
			RegisteredRewardsCount::get(),
			0,
			"Zero relayer reward should not be registered"
		);
	});
}

#[test]
fn test_add_tip_cumulative() {
	new_tester().execute_with(|| {
		let nonce: u64 = 10;
		let amount1: u128 = 500;
		let amount2: u128 = 300;

		assert_eq!(Tips::<Test>::get(nonce), None);
		assert_ok!(InboundQueue::add_tip(nonce, amount1));
		assert_eq!(Tips::<Test>::get(nonce), Some(amount1));
		assert_ok!(InboundQueue::add_tip(nonce, amount2));
		assert_eq!(Tips::<Test>::get(nonce), Some(amount1 + amount2));
	});
}

#[test]
fn test_add_tip_nonce_consumed() {
	new_tester().execute_with(|| {
		let nonce: u64 = 20;
		let amount: u128 = 400;
		Nonce::<Test>::set(nonce.into());

		assert_noop!(InboundQueue::add_tip(nonce, amount), AddTipError::NonceConsumed);
		assert_eq!(Tips::<Test>::get(nonce), None);
	});
}

#[test]
fn test_add_tip_amount_zero() {
	new_tester().execute_with(|| {
		let nonce: u64 = 30;
		let amount: u128 = 0;

		assert_noop!(InboundQueue::add_tip(nonce, amount), AddTipError::AmountZero);
		assert_eq!(Tips::<Test>::get(nonce), None);
	});
}

#[test]
fn inbound_tip_is_paid_out_to_relayer() {
	new_tester().execute_with(|| {
		let nonce: u64 = 77;
		let tip: u128 = 12_345;
		let relayer_fee: u128 = 2_000;

		// Add tip for nonce before message is processed
		assert_ok!(InboundQueue::add_tip(nonce, tip));
		assert_eq!(Tips::<Test>::get(nonce), Some(tip));

		// Process inbound message with relayer_fee
		let relayer: AccountId = Keyring::Bob.into();
		assert_ok!(InboundQueue::process_message(
			relayer,
			Message {
				nonce,
				assets: vec![],
				payload: Payload::Raw(vec![]),
				claimer: None,
				execution_fee: 1_000_000_000,
				relayer_fee,
				gateway: mock::GatewayAddress::get(),
				origin: H160::random(),
				value: 3_000_000_000,
			},
		));

		// Reward should be registered from relayer_fee + tip
		assert_eq!(
			RegisteredRewardsCount::get(),
			1,
			"Reward should be registered from relayer_fee + tip"
		);

		// Check the actual reward amount paid out (should be relayer_fee + tip)
		assert_eq!(
			RegisteredRewardAmount::get(),
			relayer_fee + tip,
			"Reward amount should equal relayer_fee + tip"
		);

		// Tip should be consumed from storage
		assert_eq!(Tips::<Test>::get(nonce), None);
	});
}

#[test]
fn relayer_fee_paid_out_when_no_tip_exists() {
	new_tester().execute_with(|| {
		let nonce: u64 = 88;
		let relayer_fee: u128 = 5_000;

		// Ensure no tip exists for this nonce
		assert_eq!(Tips::<Test>::get(nonce), None);

		// Process inbound message with relayer_fee but no tip
		let relayer: AccountId = Keyring::Bob.into();
		assert_ok!(InboundQueue::process_message(
			relayer,
			Message {
				nonce,
				assets: vec![],
				payload: Payload::Raw(vec![]),
				claimer: None,
				execution_fee: 1_000_000_000,
				relayer_fee,
				gateway: mock::GatewayAddress::get(),
				origin: H160::random(),
				value: 3_000_000_000,
			},
		));

		// Relayer fee should be paid out even without tip
		assert_eq!(
			RegisteredRewardsCount::get(),
			1,
			"Relayer fee should be paid out even when no tip exists"
		);

		// Check the actual reward amount paid out
		assert_eq!(
			RegisteredRewardAmount::get(),
			relayer_fee,
			"Reward amount should equal relayer_fee when no tip exists"
		);

		// Confirm no tip storage was affected
		assert_eq!(Tips::<Test>::get(nonce), None);
	});
}

#[test]
fn tip_paid_out_when_no_relayer_fee() {
	new_tester().execute_with(|| {
		let nonce: u64 = 99;
		let tip: u128 = 8_500;

		// Add tip for nonce before message is processed
		assert_ok!(InboundQueue::add_tip(nonce, tip));
		assert_eq!(Tips::<Test>::get(nonce), Some(tip));

		// Process inbound message with zero relayer_fee but with tip
		let relayer: AccountId = Keyring::Bob.into();
		assert_ok!(InboundQueue::process_message(
			relayer,
			Message {
				nonce,
				assets: vec![],
				payload: Payload::Raw(vec![]),
				claimer: None,
				execution_fee: 1_000_000_000,
				relayer_fee: 0,
				gateway: mock::GatewayAddress::get(),
				origin: H160::random(),
				value: 3_000_000_000,
			},
		));

		// Tip should be paid out even without relayer fee
		assert_eq!(
			RegisteredRewardsCount::get(),
			1,
			"Tip should be paid out even when relayer_fee is 0"
		);

		// Check the actual reward amount paid out (should be just the tip)
		assert_eq!(
			RegisteredRewardAmount::get(),
			tip,
			"Reward amount should equal tip when relayer_fee is 0"
		);

		// Tip should be consumed from storage
		assert_eq!(Tips::<Test>::get(nonce), None);
	});
}
