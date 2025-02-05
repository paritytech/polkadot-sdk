// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use super::*;

use crate::{mock::*, Error};
use frame_support::{assert_err, assert_noop, assert_ok};
use hex_literal::hex;
use snowbridge_inbound_queue_primitives::{EventProof, Proof};
use sp_keyring::sr25519::Keyring;
use sp_runtime::DispatchError;

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
		assert!(
			events.iter().any(|event| matches!(
				event.event,
				RuntimeEvent::InboundQueue(Event::FeesPaid { .. })
			)),
			"no fees paid event emitted."
		);
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
		event.event_log.address = hex!("0000000000000000000000000000000000000911").into();

		assert_err!(
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

		assert_err!(
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

		assert_err!(
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
	crate::test::mock_xcm_send_failure::new_tester().execute_with(|| {
		let relayer: AccountId = Keyring::Bob.into();

		let origin = mock::mock_xcm_send_failure::RuntimeOrigin::signed(relayer.clone());

		// Submit message
		let event = EventProof {
			event_log: mock_event_log(),
			proof: Proof {
				receipt_proof: Default::default(),
				execution_proof: mock_execution_proof(),
			},
		};

		assert_err!(
			crate::test::mock_xcm_send_failure::InboundQueue::submit(
				origin.clone(),
				Box::new(event.clone())
			),
			Error::<Test>::SendFailure
		);
	});
}

#[test]
fn test_xcm_send_validate_failure() {
	crate::test::mock_xcm_validate_failure::new_tester().execute_with(|| {
		let relayer: AccountId = Keyring::Bob.into();

		let origin = mock::mock_xcm_validate_failure::RuntimeOrigin::signed(relayer.clone());

		// Submit message
		let event = EventProof {
			event_log: mock_event_log(),
			proof: Proof {
				receipt_proof: Default::default(),
				execution_proof: mock_execution_proof(),
			},
		};

		assert_err!(
			crate::test::mock_xcm_validate_failure::InboundQueue::submit(
				origin.clone(),
				Box::new(event.clone())
			),
			Error::<Test>::Unreachable
		);
	});
}

#[test]
fn test_xcm_charge_fees_failure() {
	crate::test::mock_charge_fees_failure::new_tester().execute_with(|| {
		let relayer: AccountId = Keyring::Bob.into();

		let origin = mock::mock_charge_fees_failure::RuntimeOrigin::signed(relayer.clone());

		// Submit message
		let event = EventProof {
			event_log: mock_event_log(),
			proof: Proof {
				receipt_proof: Default::default(),
				execution_proof: mock_execution_proof(),
			},
		};

		assert_err!(
			crate::test::mock_charge_fees_failure::InboundQueue::submit(
				origin.clone(),
				Box::new(event.clone())
			),
			Error::<Test>::FeesNotMet
		);
	});
}
