// Copyright 2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Tests for the im-online module.

#![cfg(test)]

use super::*;
use crate::mock::*;
use offchain::testing::TestOffchainExt;
use primitives::offchain::OpaquePeerId;
use runtime_io::with_externalities;
use support::{dispatch, assert_noop};
use sr_primitives::testing::UintAuthorityId;


#[test]
fn test_unresponsiveness_slash_fraction() {
	// A single case of unresponsiveness is not slashed.
	assert_eq!(
		UnresponsivenessOffence::<()>::slash_fraction(1, 50),
		Perbill::zero(),
	);

	assert_eq!(
		UnresponsivenessOffence::<()>::slash_fraction(3, 50),
		Perbill::from_parts(6000000), // 0.6%
	);

	// One third offline should be punished around 5%.
	assert_eq!(
		UnresponsivenessOffence::<()>::slash_fraction(17, 50),
		Perbill::from_parts(48000000), // 4.8%
	);
}

#[test]
fn should_report_offline_validators() {
	with_externalities(&mut new_test_ext(), || {
		// given
		let block = 1;
		System::set_block_number(block);
		// buffer new validators
		Session::rotate_session();
		// enact the change and buffer another one
		let validators = vec![1, 2, 3, 4, 5, 6];
		VALIDATORS.with(|l| *l.borrow_mut() = Some(validators.clone()));
		Session::rotate_session();

		// when
		// we end current session and start the next one
		Session::rotate_session();

		// then
		let offences = OFFENCES.with(|l| l.replace(vec![]));
		assert_eq!(offences, vec![
			(vec![], UnresponsivenessOffence {
				session_index: 2,
				validator_set_count: 3,
				offenders: vec![
					(1, 1),
					(2, 2),
					(3, 3),
				],
			})
		]);

		// should not report when heartbeat is sent
		for (idx, v) in validators.into_iter().take(4).enumerate() {
			let _ = heartbeat(block, 3, idx as u32, v.into()).unwrap();
		}
		Session::rotate_session();

		// then
		let offences = OFFENCES.with(|l| l.replace(vec![]));
		assert_eq!(offences, vec![
			(vec![], UnresponsivenessOffence {
				session_index: 3,
				validator_set_count: 6,
				offenders: vec![
					(5, 5),
					(6, 6),
				],
			})
		]);
	});
}

fn heartbeat(
	block_number: u64,
	session_index: u32,
	authority_index: u32,
	id: UintAuthorityId,
) -> dispatch::Result {
	let heartbeat = Heartbeat {
		block_number,
		network_state: OpaqueNetworkState {
			peer_id: OpaquePeerId(vec![1]),
			external_addresses: vec![],
		},
		session_index,
		authority_index,
	};
	let signature = id.sign(&heartbeat.encode()).unwrap();

	ImOnline::heartbeat(
		Origin::system(system::RawOrigin::None),
		heartbeat,
		signature
	)
}

#[test]
fn should_mark_online_validator_when_heartbeat_is_received() {
	with_externalities(&mut new_test_ext(), || {
		advance_session();
		// given
		VALIDATORS.with(|l| *l.borrow_mut() = Some(vec![1, 2, 3, 4, 5, 6]));
		assert_eq!(Session::validators(), Vec::<u64>::new());
		// enact the change and buffer another one
		advance_session();

		assert_eq!(Session::current_index(), 2);
		assert_eq!(Session::validators(), vec![1, 2, 3]);

		assert!(!ImOnline::is_online_in_current_session(0));
		assert!(!ImOnline::is_online_in_current_session(1));
		assert!(!ImOnline::is_online_in_current_session(2));

		// when
		let _ = heartbeat(1, 2, 0, 1.into()).unwrap();

		// then
		assert!(ImOnline::is_online_in_current_session(0));
		assert!(!ImOnline::is_online_in_current_session(1));
		assert!(!ImOnline::is_online_in_current_session(2));

		// and when
		let _ = heartbeat(1, 2, 2, 3.into()).unwrap();

		// then
		assert!(ImOnline::is_online_in_current_session(0));
		assert!(!ImOnline::is_online_in_current_session(1));
		assert!(ImOnline::is_online_in_current_session(2));
	});
}

#[test]
fn late_heartbeat_should_fail() {
	with_externalities(&mut new_test_ext(), || {
		advance_session();
		// given
		VALIDATORS.with(|l| *l.borrow_mut() = Some(vec![1, 2, 4, 4, 5, 6]));
		assert_eq!(Session::validators(), Vec::<u64>::new());
		// enact the change and buffer another one
		advance_session();

		assert_eq!(Session::current_index(), 2);
		assert_eq!(Session::validators(), vec![1, 2, 3]);

		// when
		assert_noop!(heartbeat(1, 3, 0, 1.into()), "Outdated heartbeat received.");
		assert_noop!(heartbeat(1, 1, 0, 1.into()), "Outdated heartbeat received.");
	});
}

#[test]
fn should_generate_heartbeats() {
	let mut ext = new_test_ext();
	let (offchain, state) = TestOffchainExt::new();
	ext.set_offchain_externalities(offchain);

	with_externalities(&mut ext, || {
		// given
		let block = 1;
		System::set_block_number(block);
		// buffer new validators
		Session::rotate_session();
		// enact the change and buffer another one
		VALIDATORS.with(|l| *l.borrow_mut() = Some(vec![1, 2, 3, 4, 5, 6]));
		Session::rotate_session();

		// when
		UintAuthorityId::set_all_keys(vec![0, 1, 2]);
		ImOnline::offchain(2);

		// then
		let transaction = state.write().transactions.pop().unwrap();
		// All validators have `0` as their session key, so we generate 3 transactions.
		assert_eq!(state.read().transactions.len(), 2);
		// check stuff about the transaction.
		let ex: Extrinsic = Decode::decode(&mut &*transaction).unwrap();
		let heartbeat = match ex.1 {
			crate::mock::Call::ImOnline(crate::Call::heartbeat(h, _)) => h,
			e => panic!("Unexpected call: {:?}", e),
		};

		assert_eq!(heartbeat, Heartbeat {
			block_number: 2,
			network_state: runtime_io::network_state().unwrap(),
			session_index: 2,
			authority_index: 2,
		});
	});
}

#[test]
fn should_cleanup_received_heartbeats_on_session_end() {
	with_externalities(&mut new_test_ext(), || {
		advance_session();

		VALIDATORS.with(|l| *l.borrow_mut() = Some(vec![1, 2, 3]));
		assert_eq!(Session::validators(), Vec::<u64>::new());

		// enact the change and buffer another one
		advance_session();

		assert_eq!(Session::current_index(), 2);
		assert_eq!(Session::validators(), vec![1, 2, 3]);

		// send an heartbeat from authority id 0 at session 2
		let _ = heartbeat(1, 2, 0, 1.into()).unwrap();

		// the heartbeat is stored
		assert!(!ImOnline::received_heartbeats(&2, &0).is_empty());

		advance_session();

		// after the session has ended we have already processed the heartbeat
		// message, so any messages received on the previous session should have
		// been pruned.
		assert!(ImOnline::received_heartbeats(&2, &0).is_empty());
	});
}
