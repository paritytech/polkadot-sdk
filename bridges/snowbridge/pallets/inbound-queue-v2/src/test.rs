// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use super::*;

use frame_support::{assert_noop, assert_ok};
use hex_literal::hex;
use snowbridge_core::inbound::Proof;
use sp_keyring::AccountKeyring as Keyring;
use sp_runtime::DispatchError;

use crate::{mock::*, Error, Event as InboundQueueEvent};
use codec::DecodeLimit;
use snowbridge_router_primitives::inbound::v2::Asset;
use sp_core::H256;
use xcm::{
	opaque::latest::prelude::{ClearOrigin, ReceiveTeleportedAsset},
	prelude::*,
	VersionedXcm, MAX_XCM_DECODE_DEPTH,
};

#[test]
fn test_submit_happy_path() {
	new_tester().execute_with(|| {
		let relayer: AccountId = Keyring::Bob.into();

		let origin = RuntimeOrigin::signed(relayer.clone());

		// Submit message
		let message = Message {
			event_log: mock_event_log(),
			proof: Proof {
				receipt_proof: Default::default(),
				execution_proof: mock_execution_proof(),
			},
		};

		assert_ok!(InboundQueue::submit(origin.clone(), message.clone()));
		expect_events(vec![InboundQueueEvent::MessageReceived {
			nonce: 1,
			message_id: [
				183, 243, 1, 130, 170, 254, 104, 45, 116, 181, 146, 237, 14, 139, 138, 89, 43, 166,
				182, 24, 163, 222, 112, 238, 215, 83, 21, 160, 24, 88, 112, 9,
			],
		}
		.into()]);
	});
}

#[test]
fn test_submit_with_invalid_gateway() {
	new_tester().execute_with(|| {
		let relayer: AccountId = Keyring::Bob.into();
		let origin = RuntimeOrigin::signed(relayer);

		// Submit message
		let message = Message {
			event_log: mock_event_log_invalid_gateway(),
			proof: Proof {
				receipt_proof: Default::default(),
				execution_proof: mock_execution_proof(),
			},
		};
		assert_noop!(
			InboundQueue::submit(origin.clone(), message.clone()),
			Error::<Test>::InvalidGateway
		);
	});
}

#[test]
fn test_submit_with_invalid_nonce() {
	new_tester().execute_with(|| {
		let relayer: AccountId = Keyring::Bob.into();
		let origin = RuntimeOrigin::signed(relayer);

		// Submit message
		let message = Message {
			event_log: mock_event_log(),
			proof: Proof {
				receipt_proof: Default::default(),
				execution_proof: mock_execution_proof(),
			},
		};
		assert_ok!(InboundQueue::submit(origin.clone(), message.clone()));

		// Submit the same again
		assert_noop!(
			InboundQueue::submit(origin.clone(), message.clone()),
			Error::<Test>::InvalidNonce
		);
	});
}

#[test]
fn test_set_operating_mode() {
	new_tester().execute_with(|| {
		let relayer: AccountId = Keyring::Bob.into();
		let origin = RuntimeOrigin::signed(relayer);
		let message = Message {
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

		assert_noop!(InboundQueue::submit(origin, message), Error::<Test>::Halted);
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
fn test_send_native_erc20_token_payload() {
	new_tester().execute_with(|| {
        // To generate test data: forge test --match-test testSendEther  -vvvv
        let payload = hex!("29e3b139f4393adda86303fcdaa35f60bb7092bf0030ef7dba020000000000000000000004005615deb798bb3e4dfa0139dfa1b3d433cc23b72f0000b2d3595bf00600000000000000000000").to_vec();
        let message = MessageV2::decode(&mut payload.as_ref());
        assert_ok!(message.clone());

        let inbound_message = message.unwrap();

        let expected_origin: H160 = hex!("29e3b139f4393adda86303fcdaa35f60bb7092bf").into();
        let expected_token_id: H160 = hex!("5615deb798bb3e4dfa0139dfa1b3d433cc23b72f").into();
        let expected_value = 500000000000000000u128;
        let expected_xcm: Vec<u8> = vec![];
        let expected_claimer: Option<Vec<u8>> = None;

        assert_eq!(expected_origin, inbound_message.origin);
        assert_eq!(1, inbound_message.assets.len());
        if let Asset::NativeTokenERC20 { token_id, value } = &inbound_message.assets[0] {
            assert_eq!(expected_token_id, *token_id);
            assert_eq!(expected_value, *value);
        } else {
            panic!("Expected NativeTokenERC20 asset");
        }
        assert_eq!(expected_xcm, inbound_message.xcm);
        assert_eq!(expected_claimer, inbound_message.claimer);
    });
}

#[test]
fn test_send_foreign_erc20_token_payload() {
	new_tester().execute_with(|| {
        let payload = hex!("29e3b139f4393adda86303fcdaa35f60bb7092bf0030ef7dba0200000000000000000000040197874824853fb4ad04794ccfd1cc8d2a7463839cfcbc6a315a1045c60ab85f400000b2d3595bf00600000000000000000000").to_vec();
        let message = MessageV2::decode(&mut payload.as_ref());
        assert_ok!(message.clone());

       	let inbound_message = message.unwrap();

        let expected_fee = 3_000_000_000_000u128;
        let expected_origin: H160 = hex!("29e3b139f4393adda86303fcdaa35f60bb7092bf").into();
        let expected_token_id: H256 = hex!("97874824853fb4ad04794ccfd1cc8d2a7463839cfcbc6a315a1045c60ab85f40").into();
        let expected_value = 500000000000000000u128;
        let expected_xcm: Vec<u8> = vec![];
        let expected_claimer: Option<Vec<u8>> = None;

        assert_eq!(expected_origin, inbound_message.origin);
        assert_eq!(expected_fee, inbound_message.fee);
        assert_eq!(1, inbound_message.assets.len());
        if let Asset::ForeignTokenERC20 { token_id, value } = &inbound_message.assets[0] {
            assert_eq!(expected_token_id, *token_id);
            assert_eq!(expected_value, *value);
        } else {
            panic!("Expected ForeignTokenERC20 asset");
        }
        assert_eq!(expected_xcm, inbound_message.xcm);
        assert_eq!(expected_claimer, inbound_message.claimer);
    });
}

#[test]
fn test_register_token_inbound_message_with_xcm_and_claimer() {
	new_tester().execute_with(|| {
        let payload = hex!("5991a2df15a8f6a256d3ec51e99254cd3fb576a90030ef7dba020000000000000000000004005615deb798bb3e4dfa0139dfa1b3d433cc23b72f00000000000000000000000000000000300508020401000002286bee0a015029e3b139f4393adda86303fcdaa35f60bb7092bf").to_vec();
        let message = MessageV2::decode(&mut payload.as_ref());
        assert_ok!(message.clone());

        let inbound_message = message.unwrap();

        let expected_origin: H160 = hex!("5991a2df15a8f6a256d3ec51e99254cd3fb576a9").into();
        let expected_token_id: H160 = hex!("5615deb798bb3e4dfa0139dfa1b3d433cc23b72f").into();
        let expected_value = 0u128;
        let expected_xcm: Vec<u8> = hex!("0508020401000002286bee0a").to_vec();
        let expected_claimer: Option<Vec<u8>> = Some(hex!("29E3b139f4393aDda86303fcdAa35F60Bb7092bF").to_vec());

        assert_eq!(expected_origin, inbound_message.origin);
        assert_eq!(1, inbound_message.assets.len());
        if let Asset::NativeTokenERC20 { token_id, value } = &inbound_message.assets[0] {
            assert_eq!(expected_token_id, *token_id);
            assert_eq!(expected_value, *value);
        } else {
            panic!("Expected NativeTokenERC20 asset");
        }
        assert_eq!(expected_xcm, inbound_message.xcm);
        assert_eq!(expected_claimer, inbound_message.claimer);

        // decode xcm
        let versioned_xcm = VersionedXcm::<()>::decode_with_depth_limit(
            MAX_XCM_DECODE_DEPTH,
            &mut inbound_message.xcm.as_ref(),
        );

        assert_ok!(versioned_xcm.clone());

        // Check if decoding was successful
        let decoded_instructions = match versioned_xcm.unwrap() {
            VersionedXcm::V5(decoded) => decoded,
            _ => {
                panic!("unexpected xcm version found")
            }
        };

        let mut decoded_instructions = decoded_instructions.into_iter();
        let decoded_first = decoded_instructions.next().take();
        assert!(decoded_first.is_some());
        let decoded_second = decoded_instructions.next().take();
        assert!(decoded_second.is_some());
        assert_eq!(ClearOrigin, decoded_second.unwrap(), "Second instruction (ClearOrigin) does not match.");
    });
}

#[test]
fn encode_xcm() {
	new_tester().execute_with(|| {
		let total_fee_asset: xcm::opaque::latest::Asset =
			(Location::parent(), 1_000_000_000).into();

		let instructions: Xcm<()> =
			vec![ReceiveTeleportedAsset(total_fee_asset.into()), ClearOrigin].into();

		let versioned_xcm_message = VersionedXcm::V5(instructions.clone());

		let xcm_bytes = VersionedXcm::encode(&versioned_xcm_message);
		let hex_string = hex::encode(xcm_bytes.clone());

		println!("xcm hex: {}", hex_string);

		let versioned_xcm = VersionedXcm::<()>::decode_with_depth_limit(
			MAX_XCM_DECODE_DEPTH,
			&mut xcm_bytes.as_ref(),
		);

		assert_ok!(versioned_xcm.clone());

		// Check if decoding was successful
		let decoded_instructions = match versioned_xcm.unwrap() {
			VersionedXcm::V5(decoded) => decoded,
			_ => {
				panic!("unexpected xcm version found")
			},
		};

		let mut original_instructions = instructions.into_iter();
		let mut decoded_instructions = decoded_instructions.into_iter();

		let original_first = original_instructions.next().take();
		let decoded_first = decoded_instructions.next().take();
		assert_eq!(
			original_first, decoded_first,
			"First instruction (ReceiveTeleportedAsset) does not match."
		);

		let original_second = original_instructions.next().take();
		let decoded_second = decoded_instructions.next().take();
		assert_eq!(
			original_second, decoded_second,
			"Second instruction (ClearOrigin) does not match."
		);
	});
}
