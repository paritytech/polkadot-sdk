// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

#![cfg(test)]

use super::*;
use crate::mock::*;
use codec::Encode;
use cumulus_pallet_parachain_system::relay_state_snapshot::ProcessRelayProofKeys;
use cumulus_primitives_core::ParaId;
use frame_support::assert_ok;

#[test]
fn process_relay_proof_keys_with_new_data_calls_handler() {
	new_test_ext().execute_with(|| {
		ReceivedData::set(vec![]);
		let publisher = ParaId::from(1000);
		let key = vec![0x12, 0x34];
		let value = vec![0xAA, 0xBB].encode();

		TestSubscriptions::set(vec![(publisher, vec![key.clone()])]);

		let proof = build_sproof_with_child_data(publisher, vec![(key.clone(), value.clone())]);

		Pallet::<Test>::process_relay_proof_keys(&proof);

		let received = ReceivedData::get();
		assert_eq!(received.len(), 1);
		assert_eq!(received[0].0, publisher);
		assert_eq!(received[0].1, key);
		assert_eq!(received[0].2, Vec::<u8>::decode(&mut &value[..]).unwrap());
	});
}

#[test]
fn process_empty_subscriptions() {
	new_test_ext().execute_with(|| {
		ReceivedData::set(vec![]);
		TestSubscriptions::set(vec![]);

		let proof = build_sproof_with_child_data(ParaId::from(1000), vec![]);

		Pallet::<Test>::process_relay_proof_keys(&proof);

		assert_eq!(ReceivedData::get().len(), 0);
	});
}

#[test]
fn root_change_triggers_processing() {
	new_test_ext().execute_with(|| {
		ReceivedData::set(vec![]);
		let publisher = ParaId::from(1000);
		let key = vec![0x01];
		let value1 = vec![0x11].encode();
		let value2 = vec![0x22].encode();

		TestSubscriptions::set(vec![(publisher, vec![key.clone()])]);

		// First block
		let proof1 = build_sproof_with_child_data(publisher, vec![(key.clone(), value1.clone())]);
		Pallet::<Test>::process_relay_proof_keys(&proof1);
		assert_eq!(ReceivedData::get().len(), 1);

		// Second block with different value (root changed)
		ReceivedData::set(vec![]);
		let proof2 = build_sproof_with_child_data(publisher, vec![(key.clone(), value2.clone())]);
		Pallet::<Test>::process_relay_proof_keys(&proof2);

		assert_eq!(ReceivedData::get().len(), 1);
		assert_eq!(ReceivedData::get()[0].2, Vec::<u8>::decode(&mut &value2[..]).unwrap());
	});
}

#[test]
fn unchanged_root_skips_processing() {
	new_test_ext().execute_with(|| {
		ReceivedData::set(vec![]);
		let publisher = ParaId::from(1000);
		let key = vec![0x01];
		let value = vec![0x11].encode();

		TestSubscriptions::set(vec![(publisher, vec![key.clone()])]);

		// First block
		let proof = build_sproof_with_child_data(publisher, vec![(key.clone(), value.clone())]);
		Pallet::<Test>::process_relay_proof_keys(&proof);
		assert_eq!(ReceivedData::get().len(), 1);

		// Second block with same data (unchanged root)
		ReceivedData::set(vec![]);
		let proof2 = build_sproof_with_child_data(publisher, vec![(key.clone(), value)]);
		Pallet::<Test>::process_relay_proof_keys(&proof2);

		assert_eq!(ReceivedData::get().len(), 0, "Handler should not be called for unchanged root");
	});
}

#[test]
fn clear_stored_roots_extrinsic() {
	new_test_ext().execute_with(|| {
		let publisher = ParaId::from(1000);
		TestSubscriptions::set(vec![(publisher, vec![vec![0x01]])]);

		// Store some roots
		let proof = build_sproof_with_child_data(publisher, vec![(vec![0x01], vec![0x11].encode())]);
		Pallet::<Test>::process_relay_proof_keys(&proof);

		assert!(!PreviousPublishedDataRoots::<Test>::get().is_empty());

		// Clear roots
		assert_ok!(Pallet::<Test>::clear_stored_roots(frame_system::RawOrigin::Root.into()));

		assert!(PreviousPublishedDataRoots::<Test>::get().is_empty());
	});
}

#[test]
fn data_processed_event_emitted() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);

		let publisher = ParaId::from(1000);
		let key = vec![0x12];
		let value = vec![0xAA].encode();

		TestSubscriptions::set(vec![(publisher, vec![key.clone()])]);

		let proof = build_sproof_with_child_data(publisher, vec![(key.clone(), value.clone())]);
		Pallet::<Test>::process_relay_proof_keys(&proof);

		// value_size is the decoded Vec<u8> length, not the encoded length
		let decoded_len = Vec::<u8>::decode(&mut &value[..]).unwrap().len() as u32;

		System::assert_has_event(
			Event::DataProcessed {
				publisher,
				key: key.try_into().unwrap(),
				value_size: decoded_len,
			}
			.into(),
		);
	});
}
