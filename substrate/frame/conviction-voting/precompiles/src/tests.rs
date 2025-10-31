// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.
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

use super::*;
use crate::{
	mock::*,
	IConvictionVoting::{self},
};
use frame_support::traits::VoteTally;
use pallet_conviction_voting::{AccountVote, Conviction, Event, TallyOf, Vote};
use pallet_revive::{
	precompiles::alloy::{
		hex,
		sol_types::{SolCall, SolInterface},
	},
	ExecConfig, ExecReturnValue, Weight, H160, U256,
};

fn tally(index: ReferendumIndex) -> TallyOf<Test> {
	<TestPolls as Polling<TallyOf<Test>>>::as_ongoing(index).expect("No poll").0
}

fn class(index: ReferendumIndex) -> TrackId {
	<TestPolls as Polling<TallyOf<Test>>>::as_ongoing(index).expect("No poll").1
}

fn call_precompile(
	from: AccountId,
	encoded_call: Vec<u8>,
) -> Result<ExecReturnValue, sp_runtime::DispatchError> {
	let precompile_addr = H160::from(
		hex::const_decode_to_array(b"00000000000000000000000000000000000C0000").unwrap(),
	);

	let result = pallet_revive::Pallet::<Test>::bare_call(
		RuntimeOrigin::signed(from),
		precompile_addr,
		U256::zero(),
		Weight::MAX,
		u128::MAX,
		encoded_call,
		ExecConfig::new_substrate_tx(),
	);

	return result.result
}

fn call_and_check_revert(from: AccountId, encoded_call: Vec<u8>) -> bool {
	let return_value = match call_precompile(from, encoded_call) {
		Ok(value) => value,
		Err(err) => panic!("ConvictionVotingPrecompile call failed with error: {err:?}"),
	};
	!return_value.did_revert()
}

fn encode_standard(
	referendum_index: ReferendumIndex,
	aye: bool,
	balance: u128,
	conviction: u8,
) -> Vec<u8> {
	let call_params = IConvictionVoting::voteStandardCall {
		referendumIndex: referendum_index,
		aye,
		conviction: conviction.try_into().unwrap(),
		balance,
	};
	let call = IConvictionVoting::IConvictionVotingCalls::voteStandard(call_params);
	call.abi_encode()
}

fn encode_split(referendum_index: ReferendumIndex, aye: Balance, nay: Balance) -> Vec<u8> {
	let call_params = IConvictionVoting::voteSplitCall {
		referendumIndex: referendum_index,
		ayeAmount: aye,
		nayAmount: nay,
	};
	let call = IConvictionVoting::IConvictionVotingCalls::voteSplit(call_params);
	call.abi_encode()
}

fn encode_split_abstain(
	referendum_index: ReferendumIndex,
	aye: Balance,
	nay: Balance,
	abstain: u128,
) -> Vec<u8> {
	let call_params = IConvictionVoting::voteSplitAbstainCall {
		referendumIndex: referendum_index,
		ayeAmount: aye,
		nayAmount: nay,
		abstainAmount: abstain,
	};
	let call = IConvictionVoting::IConvictionVotingCalls::voteSplitAbstain(call_params);
	call.abi_encode()
}

fn encode_delegate(track_id: TrackId, to: AccountId, conviction: u8, balance: Balance) -> Vec<u8> {
	let mapped_to = <Test as pallet_revive::Config>::AddressMapper::to_address(&to);
	let call_params = IConvictionVoting::delegateCall {
		trackId: track_id,
		to: mapped_to.0.into(),
		conviction: conviction.try_into().unwrap(),
		balance,
	};
	let call = IConvictionVoting::IConvictionVotingCalls::delegate(call_params);
	call.abi_encode()
}

fn encode_undelegate(track_id: TrackId) -> Vec<u8> {
	let call_params = IConvictionVoting::undelegateCall { trackId: track_id };
	let call = IConvictionVoting::IConvictionVotingCalls::undelegate(call_params);
	call.abi_encode()
}

fn encode_get_voting(
	who: AccountId,
	track_id: TrackId,
	referendum_index: ReferendumIndex,
) -> Vec<u8> {
	let mapped_who = <Test as pallet_revive::Config>::AddressMapper::to_address(&who);
	let call_params = IConvictionVoting::getVotingCall {
		who: mapped_who.0.into(),
		trackId: track_id,
		referendumIndex: referendum_index,
	};
	let call = IConvictionVoting::IConvictionVotingCalls::getVoting(call_params);
	call.abi_encode()
}

#[test]
fn test_vote_standard_encoding() {
	let referendum_index = 3u32;
	let balance = 2u128;

	let encoded_call = encode_standard(referendum_index, true, balance, 5);

	let decoded_call = IConvictionVoting::voteStandardCall::abi_decode(&encoded_call).unwrap();

	assert_eq!(decoded_call.balance, balance);
}

#[test]
fn test_vote_standard_precompile_works() {
	new_test_ext().execute_with(|| {
		let referendum_index = 3u32;
		let balance = 2u128;
		let conviction = 5u8;

		let encoded_call = encode_standard(referendum_index, true, balance, conviction);

		assert!(call_and_check_revert(ALICE, encoded_call));

		let vote = Vote { aye: true, conviction: Conviction::Locked5x };
		System::assert_last_event(tests::RuntimeEvent::ConvictionVoting(Event::Voted {
			who: ALICE,
			vote: AccountVote::Standard { vote, balance: 2u128 },
			poll_index: referendum_index,
		}));
		assert_eq!(tally(referendum_index), Tally::from_parts(10, 0, 2));

		let encoded_call = encode_standard(referendum_index, false, balance, conviction);
		assert!(call_and_check_revert(BOB, encoded_call));

		let vote = Vote { aye: false, conviction: Conviction::Locked5x };
		System::assert_last_event(tests::RuntimeEvent::ConvictionVoting(Event::Voted {
			who: BOB,
			vote: AccountVote::Standard { vote, balance: 2u128 },
			poll_index: referendum_index,
		}));
	});
}

#[test]
fn test_vote_split_encoding() {
	let referendum_index = 3u32;
	let aye_amount = 10u128;
	let nay_amount = 5u128;

	let encoded_call = encode_split(referendum_index, aye_amount, nay_amount);

	let decoded_call = IConvictionVoting::voteSplitCall::abi_decode(&encoded_call).unwrap();

	assert_eq!(decoded_call.ayeAmount, aye_amount);
}

#[test]
fn test_vote_split_works() {
	new_test_ext().execute_with(|| {
		let referendum_index = 3u32;
		let aye_amount = 10u128;
		let nay_amount = 5u128;

		let encoded_call = encode_split(referendum_index, aye_amount, nay_amount);

		assert!(call_and_check_revert(ALICE, encoded_call));

		System::assert_last_event(tests::RuntimeEvent::ConvictionVoting(Event::Voted {
			who: ALICE,
			vote: AccountVote::Split { aye: 10u128, nay: 5u128 },
			poll_index: referendum_index,
		}));
		assert_eq!(tally(referendum_index), Tally::from_parts(1, 0, 10));
	});
}

#[test]
fn test_vote_split_abstain_encoding() {
	let referendum_index = 3u32;
	let aye_amount = 10u128;
	let nay_amount = 5u128;
	let abstain_amount = 15u128;

	let encoded_call =
		encode_split_abstain(referendum_index, aye_amount, nay_amount, abstain_amount);

	let decoded_call = IConvictionVoting::voteSplitAbstainCall::abi_decode(&encoded_call).unwrap();

	assert_eq!(decoded_call.ayeAmount, aye_amount);
}

#[test]
fn test_vote_split_abstain_works() {
	new_test_ext().execute_with(|| {
		let referendum_index = 3u32;
		let aye_amount = 10u128;
		let nay_amount = 5u128;
		let abstain_amount = 15u128;

		let encoded_call =
			encode_split_abstain(referendum_index, aye_amount, nay_amount, abstain_amount);

		assert!(call_and_check_revert(ALICE, encoded_call));

		System::assert_last_event(tests::RuntimeEvent::ConvictionVoting(Event::Voted {
			who: ALICE,
			vote: AccountVote::SplitAbstain { aye: 10u128, nay: 5u128, abstain: 15u128 },
			poll_index: referendum_index,
		}));
		assert_eq!(tally(referendum_index), Tally::from_parts(1, 0, 25));
	});
}

#[test]
fn test_vote_not_ongoing_error() {
	new_test_ext().execute_with(|| {
		let referendum_index = 1u32;
		let aye_amount = 10u128;
		let nay_amount = 5u128;
		let abstain_amount = 15u128;

		let encoded_call =
			encode_split_abstain(referendum_index, aye_amount, nay_amount, abstain_amount);

		assert!(!call_and_check_revert(ALICE, encoded_call));
	})
}

#[test]
fn test_vote_insufficient_funds_error() {
	new_test_ext_with_balances(vec![(ALICE, 2u128)]).execute_with(|| {
		let referendum_index = 3u32;
		let aye_amount = 10u128;
		let nay_amount = 5u128;
		let abstain_amount = 15u128;

		let encoded_call =
			encode_split_abstain(referendum_index, aye_amount, nay_amount, abstain_amount);

		assert!(!call_and_check_revert(ALICE, encoded_call));
	})
}

#[test]
fn test_vote_already_delegating_error() {
	new_test_ext().execute_with(|| {
		Polls::set(vec![(0, TestPollState::Ongoing(Tally::new(0), 0))].into_iter().collect());

		let track_id = 0u16;
		let referendum_index = 0u32;
		let balance = 10u128;
		let conviction = 1u8;

		assert!(call_and_check_revert(
			BOB,
			encode_standard(referendum_index, true, balance, conviction)
		));

		assert!(call_and_check_revert(ALICE, encode_delegate(track_id, BOB, conviction, balance)));

		assert!(!call_and_check_revert(
			ALICE,
			encode_standard(referendum_index, true, balance, conviction)
		));
	})
}

#[test]
fn test_vote_lock_balances_works() {
	new_test_ext().execute_with(|| {
		let referendum_index = 3u32;
		let vote_balance = 2u128;
		let conviction = 5u8;

		let encoded_call = encode_standard(referendum_index, true, vote_balance, conviction);

		let prev_balance = Balances::usable_balance(ALICE);

		assert!(call_and_check_revert(ALICE, encoded_call));

		assert_eq!(Balances::usable_balance(ALICE), prev_balance.saturating_sub(vote_balance));
	});
}

#[test]
fn test_delegate_encoding() {
	let balance = 10u128;

	let encoded_call = encode_delegate(1, BOB, 1, balance);

	let decoded_call = IConvictionVoting::delegateCall::abi_decode(&encoded_call).unwrap();

	assert_eq!(decoded_call.balance, balance);
}

#[test]
fn test_delegation_precompile_works() {
	new_test_ext().execute_with(|| {
		let balance = 5u128;
		let conviction = 1u8;

		Polls::set(
			vec![
				(0, TestPollState::Ongoing(Tally::new(0), 0)),
				(1, TestPollState::Ongoing(Tally::new(0), 1)),
				(2, TestPollState::Ongoing(Tally::new(0), 2)),
				(3, TestPollState::Ongoing(Tally::new(0), 2)),
			]
			.into_iter()
			.collect(),
		);

		let prev_balance = Balances::usable_balance(ALICE);

		let targets = [BOB, CHARLIE, DAVE];
		// delegates class-wise to a different person
		for (track, to) in targets.iter().enumerate() {
			let encoded_call =
				encode_delegate(track.try_into().unwrap(), to.clone(), conviction, balance);
			assert!(call_and_check_revert(ALICE, encoded_call));
		}
		System::assert_last_event(tests::RuntimeEvent::ConvictionVoting(Event::Delegated(
			ALICE, DAVE, 2,
		)));
		assert_eq!(Balances::usable_balance(ALICE), prev_balance.saturating_sub(5u128));

		let target_voting_balance = 10u128;
		let target_voting_conviction = 0u8;
		for (to_idx, to) in targets.iter().enumerate() {
			for referendum_index in 0..=2 {
				let encoded_call = encode_standard(
					referendum_index,
					// See ../tests.rs, we want to keep test simmilar...
					to_idx == referendum_index as usize,
					target_voting_balance,
					target_voting_conviction,
				);
				assert!(call_and_check_revert(to.clone(), encoded_call));
			}
		}

		assert_eq!(
			Polls::get(),
			vec![
				(0, TestPollState::Ongoing(Tally::from_parts(6, 2, 15), 0)),
				(1, TestPollState::Ongoing(Tally::from_parts(6, 2, 15), 1)),
				(2, TestPollState::Ongoing(Tally::from_parts(6, 2, 15), 2)),
				(3, TestPollState::Ongoing(Tally::from_parts(0, 0, 0), 2)),
			]
			.into_iter()
			.collect()
		);

		// DAVE votes nay to 3
		let referendum_index = 3u32;
		let encoded_call = encode_standard(
			referendum_index,
			false,
			target_voting_balance,
			target_voting_conviction,
		);
		assert!(call_and_check_revert(DAVE, encoded_call));

		assert_eq!(
			Polls::get(),
			vec![
				(0, TestPollState::Ongoing(Tally::from_parts(6, 2, 15), 0)),
				(1, TestPollState::Ongoing(Tally::from_parts(6, 2, 15), 1)),
				(2, TestPollState::Ongoing(Tally::from_parts(6, 2, 15), 2)),
				(3, TestPollState::Ongoing(Tally::from_parts(0, 6, 0), 2)),
			]
			.into_iter()
			.collect()
		);

		// ALICE redelegates for class 2 to CHARLIE
		let track_id = 2u16;
		assert!(call_and_check_revert(ALICE, encode_undelegate(track_id)));
		assert!(call_and_check_revert(ALICE, encode_delegate(track_id, CHARLIE, 1, 5)));
		assert_eq!(
			Polls::get(),
			vec![
				(0, TestPollState::Ongoing(Tally::from_parts(6, 2, 15), 0)),
				(1, TestPollState::Ongoing(Tally::from_parts(6, 2, 15), 1)),
				(2, TestPollState::Ongoing(Tally::from_parts(1, 7, 10), 2)),
				(3, TestPollState::Ongoing(Tally::from_parts(0, 1, 0), 2)),
			]
			.into_iter()
			.collect()
		);
	});
}

#[test]
fn test_delegate_already_delegating_error() {
	new_test_ext().execute_with(|| {
		let target = BOB;
		let track_id = 0u16;
		let balance = 10u128;
		let conviction = 0u8;
		assert!(call_and_check_revert(
			ALICE,
			encode_delegate(track_id, target, conviction, balance)
		));
		assert!(!call_and_check_revert(
			ALICE,
			encode_delegate(track_id, CHARLIE, conviction, balance)
		));
	});
}

#[test]
fn test_delegate_bad_class_error() {
	new_test_ext().execute_with(|| {
		let target = BOB;
		let balance = 10u128;
		let conviction = 0u8;
		assert!(!call_and_check_revert(
			ALICE,
			encode_delegate(1010u16, target, conviction, balance)
		));
	});
}

#[test]
fn test_delegate_insufficient_funds_error() {
	new_test_ext_with_balances(vec![(ALICE, 2)]).execute_with(|| {
		let target = BOB;
		let track_id = 0u16;
		let balance = 10u128;
		let conviction = 0u8;
		assert!(!call_and_check_revert(
			ALICE,
			encode_delegate(track_id, target, conviction, balance)
		));
	});
}

#[test]
fn test_delegate_already_voting_error() {
	new_test_ext().execute_with(|| {
		Polls::set(vec![(0, TestPollState::Ongoing(Tally::new(0), 0))].into_iter().collect());

		let target = BOB;
		let track_id = 0u16;
		let referendum_index = 0u32;
		let balance = 10u128;
		let conviction = 0u8;
		assert!(call_and_check_revert(
			ALICE,
			encode_standard(referendum_index, false, balance, conviction)
		));
		assert!(!call_and_check_revert(
			ALICE,
			encode_delegate(track_id, target, conviction, balance)
		));
	});
}

#[test]
fn test_undelegate_encoding() {
	let track_id = 2u16;

	let encoded_call = encode_undelegate(track_id);

	let decoded_call = IConvictionVoting::undelegateCall::abi_decode(&encoded_call).unwrap();

	assert_eq!(decoded_call.trackId, track_id);
}

#[test]
fn test_undelegate_precompile_works() {
	new_test_ext().execute_with(|| {
		let target = BOB;
		let track_id = 0u16;
		let referendum_index = 0u32;
		let balance = 10u128;
		let conviction = 1u8;

		Polls::set(vec![(0, TestPollState::Ongoing(Tally::new(0), 0))].into_iter().collect());

		assert!(call_and_check_revert(
			BOB,
			encode_standard(referendum_index, true, balance, conviction)
		));

		assert!(call_and_check_revert(
			ALICE,
			encode_delegate(track_id, target, conviction, balance)
		));

		assert_eq!(
			Polls::get(),
			vec![(0, TestPollState::Ongoing(Tally::from_parts(20, 0, 20), 0)),]
				.into_iter()
				.collect()
		);

		assert!(call_and_check_revert(ALICE, encode_undelegate(track_id)));

		assert_eq!(
			Polls::get(),
			vec![(0, TestPollState::Ongoing(Tally::from_parts(10, 0, 10), 0)),]
				.into_iter()
				.collect()
		);
	});
}

#[test]
fn test_undelegate_not_delegating_error() {
	new_test_ext().execute_with(|| {
		let track_id = 0u16;
		assert!(!call_and_check_revert(ALICE, encode_undelegate(track_id)));
	});
}

#[test]
fn test_get_voting_encoding() {
	let who = ALICE;
	let track_id = 0;
	let referendum_index = 3;

	let encoded_call = encode_get_voting(who, track_id, referendum_index);

	let decoded_call = IConvictionVoting::getVotingCall::abi_decode(&encoded_call).unwrap();
	assert_eq!(decoded_call.trackId, track_id);
}

#[test]
fn test_get_voting_standard_precompile_work() {
	new_test_ext().execute_with(|| {
		let who = ALICE;
		let track_id = 0;
		let referendum_index = 3;
		let balance = 10u128;
		let conviction = 1u8;

		assert!(call_and_check_revert(
			ALICE,
			encode_standard(referendum_index, true, balance, conviction),
		));

		// Should return correctly when voting standard
		let return_value =
			match call_precompile(ALICE, encode_get_voting(ALICE, track_id, referendum_index)) {
				Ok(value) => value,
				Err(err) => panic!("ConvictionVotingPrecompile call failed with error: {err:?}"),
			};

		let decoded_value = match VotingOf::<Test>::abi_decode(&return_value.data) {
			Ok(value) => value,
			Err(err) => panic!("Decoding failed with error: {err:?}"),
		};

		assert_eq!(IConvictionVoting::VotingType::Standard as u8, decoded_value.1 as u8);
		assert_eq!(balance, decoded_value.3);
	});
}

#[test]
fn test_get_voting_while_delegating_precompile_work() {
	new_test_ext().execute_with(|| {
		let track_id = 0;
		let referendum_index = 3;
		let balance = 10u128;
		let conviction = 1u8;

		assert!(call_and_check_revert(
			ALICE,
			encode_standard(referendum_index, true, balance, conviction),
		));

		assert!(call_and_check_revert(BOB, encode_delegate(track_id, ALICE, conviction, balance),));

		// Should return exists false when delegating
		let return_value =
			match call_precompile(ALICE, encode_get_voting(BOB, track_id, referendum_index)) {
				Ok(value) => value,
				Err(err) => panic!("ConvictionVotingPrecompile call failed with error: {err:?}"),
			};

		let decoded_value = match VotingOf::<Test>::abi_decode(&return_value.data) {
			Ok(value) => value,
			Err(err) => panic!("Decoding failed with error: {err:?}"),
		};

		assert_eq!(false, decoded_value.0);
	});
}

#[test]
fn test_get_voting_split_precompile_work() {
	new_test_ext().execute_with(|| {
		let track_id = 0;
		let referendum_index = 3;
		let aye = 10u128;
		let nay = 11u128;

		assert!(call_and_check_revert(ALICE, encode_split(referendum_index, aye, nay),));

		// Should return correctly when voting split
		let return_value =
			match call_precompile(ALICE, encode_get_voting(ALICE, track_id, referendum_index)) {
				Ok(value) => value,
				Err(err) => panic!("ConvictionVotingPrecompile call failed with error: {err:?}"),
			};

		let decoded_value = match VotingOf::<Test>::abi_decode(&return_value.data) {
			Ok(value) => value,
			Err(err) => panic!("Decoding failed with error: {err:?}"),
		};

		assert_eq!(IConvictionVoting::VotingType::Split as u8, decoded_value.1 as u8);
		assert_eq!(aye, decoded_value.3);
		assert_eq!(nay, decoded_value.4);
	});
}

#[test]
fn test_get_voting_split_abstain_precompile_work() {
	new_test_ext().execute_with(|| {
		let track_id = 0;
		let referendum_index = 3;
		let aye = 10u128;
		let nay = 11u128;
		let abstain = 12u128;

		assert!(call_and_check_revert(
			ALICE,
			encode_split_abstain(referendum_index, aye, nay, abstain),
		));

		// Should return correctly when voting split abstain
		let return_value =
			match call_precompile(ALICE, encode_get_voting(ALICE, track_id, referendum_index)) {
				Ok(value) => value,
				Err(err) => panic!("ConvictionVotingPrecompile call failed with error: {err:?}"),
			};

		let decoded_value = match VotingOf::<Test>::abi_decode(&return_value.data) {
			Ok(value) => value,
			Err(err) => panic!("Decoding failed with error: {err:?}"),
		};

		assert_eq!(IConvictionVoting::VotingType::SplitAbstain as u8, decoded_value.1 as u8);
		assert_eq!(aye, decoded_value.3);
		assert_eq!(nay, decoded_value.4);
		assert_eq!(abstain, decoded_value.5);
	});
}

#[test]
fn test_get_voting_no_voting_work() {
	new_test_ext().execute_with(|| {
		let track_id = 0;
		let referendum_index = 3;

		// Should return correctly when no voting
		let return_value =
			match call_precompile(ALICE, encode_get_voting(ALICE, track_id, referendum_index)) {
				Ok(value) => value,
				Err(err) => panic!("ConvictionVotingPrecompile call failed with error: {err:?}"),
			};

		let decoded_value = match VotingOf::<Test>::abi_decode(&return_value.data) {
			Ok(value) => value,
			Err(err) => panic!("Decoding failed with error: {err:?}"),
		};

		assert_eq!(false, decoded_value.0);
	});
}
