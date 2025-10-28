use super::*;
use crate::{
	mock::*,
	IConvictionVoting::{self},
};
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

fn encode_aye(referendum_index: ReferendumIndex, balance: u128, conviction: u8) -> Vec<u8> {
	let call_params = IConvictionVoting::voteStandardCall {
		referendumIndex: referendum_index,
		aye: true,
		conviction: conviction.try_into().unwrap(),
		balance,
	};
	let call = IConvictionVoting::IConvictionVotingCalls::voteStandard(call_params);
	call.abi_encode()
}

fn encode_nay(referendum_index: ReferendumIndex, balance: u128, conviction: u8) -> Vec<u8> {
	let call_params = IConvictionVoting::voteStandardCall {
		referendumIndex: referendum_index,
		aye: false,
		conviction: conviction.try_into().unwrap(),
		balance,
	};
	let call = IConvictionVoting::IConvictionVotingCalls::voteStandard(call_params);
	call.abi_encode()
}

fn encode_split(referendum_index: ReferendumIndex, aye: u128, nay: u128) -> Vec<u8> {
	let call_params = IConvictionVoting::voteSplitCall {
		referendumIndex: referendum_index,
		ayeAmount: aye,
		nayAmount: nay,
	};
	let call = IConvictionVoting::IConvictionVotingCalls::voteSplit(call_params);
	call.abi_encode()
}

fn encode_split_abstain(referendum_index: ReferendumIndex, aye: u128, nay: u128, abstain: u128) -> Vec<u8> {
	let call_params = IConvictionVoting::voteSplitAbstainCall {
		referendumIndex: referendum_index,
		ayeAmount: aye,
		nayAmount: nay,
		abstainAmount: abstain
	};
	let call = IConvictionVoting::IConvictionVotingCalls::voteSplitAbstain(call_params);
	call.abi_encode()
}

#[test]
fn test_vote_standard_encoding() {
	let referendum_index = 3u32;
	let balance = 2u128;

	let encoded_call = encode_aye(referendum_index, balance, 5);

	let decoded_call = IConvictionVoting::voteStandardCall::abi_decode(&encoded_call).unwrap();

	assert_eq!(decoded_call.balance, balance);
}

#[test]
fn test_vote_standard_precompile_works() {
	new_test_ext().execute_with(|| {
		let referendum_index = 3u32;
		let balance = 2u128;
		let conviction = 5u8;

		let encoded_call = encode_aye(referendum_index, balance, conviction);

		assert!(call_precompile(ALICE, encoded_call).is_ok());

		let vote = Vote { aye: true, conviction: Conviction::Locked5x };
		System::assert_last_event(tests::RuntimeEvent::ConvictionVoting(Event::Voted {
			who: ALICE,
			vote: AccountVote::Standard { vote, balance: 2u128 },
			poll_index: referendum_index,
		}));
		assert_eq!(tally(referendum_index), Tally::from_parts(10, 0, 2));

		let encoded_call = encode_nay(referendum_index, balance, conviction);
		assert!(call_precompile(BOB, encoded_call).is_ok());

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
	new_test_ext().execute_with(|| {
		let referendum_index = 3u32;
		let aye_amount = 10u128;
		let nay_amount = 5u128;


		let encoded_call = encode_split(referendum_index, aye_amount, nay_amount);

		let decoded_call = IConvictionVoting::voteSplitCall::abi_decode(&encoded_call).unwrap();

		assert_eq!(decoded_call.ayeAmount, aye_amount);
	});
}

#[test]
fn test_vote_split_works() {
	new_test_ext().execute_with(|| {
		let referendum_index = 3u32;
		let aye_amount = 10u128;
		let nay_amount = 5u128;

		let encoded_call = encode_split(referendum_index, aye_amount, nay_amount);

		assert!(call_precompile(ALICE, encoded_call).is_ok());

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
	new_test_ext().execute_with(|| {
		let referendum_index = 3u32;
		let aye_amount = 10u128;
		let nay_amount = 5u128;
		let abstain_amount = 15u128;


		let encoded_call = encode_split_abstain(referendum_index, aye_amount, nay_amount, abstain_amount);

		let decoded_call =
			IConvictionVoting::voteSplitAbstainCall::abi_decode(&encoded_call).unwrap();

		assert_eq!(decoded_call.ayeAmount, aye_amount);
	});
}

#[test]
fn test_vote_split_abstain_works() {
	new_test_ext().execute_with(|| {
		let referendum_index = 3u32;
		let aye_amount = 10u128;
		let nay_amount = 5u128;
		let abstain_amount = 15u128;

		
		let encoded_call = encode_split_abstain(referendum_index, aye_amount, nay_amount, abstain_amount);

		assert!(call_precompile(ALICE, encoded_call).is_ok());

		System::assert_last_event(tests::RuntimeEvent::ConvictionVoting(Event::Voted {
			who: ALICE,
			vote: AccountVote::SplitAbstain { aye: 10u128, nay: 5u128, abstain: 15u128 },
			poll_index: referendum_index,
		}));
		assert_eq!(tally(referendum_index), Tally::from_parts(1, 0, 25));
	});
}

#[test]
fn test_vote_not_ongoing_fails() {
	new_test_ext().execute_with(|| {
		let referendum_index = 1u32;
		let aye_amount = 10u128;
		let nay_amount = 5u128;
		let abstain_amount = 15u128;


		let encoded_call = encode_split_abstain(referendum_index, aye_amount, nay_amount, abstain_amount);
		let return_value = match call_precompile(ALICE, encoded_call) {
			Ok(value) => value,
			Err(err) => panic!("ConvictionVotingPrecompile call failed with error: {err:?}"),
		};

		assert!(return_value.did_revert());
	})
}

#[test]
fn test_vote_lock_balances_works() {
	new_test_ext().execute_with(|| {
		let referendum_index = 3u32;
		let vote_balance = 2u128;
		let conviction = 5u8;

		let encoded_call = encode_aye(referendum_index, vote_balance, conviction);

		let prev_balance = Balances::usable_balance(ALICE);

		assert!(call_precompile(ALICE, encoded_call).is_ok());

		assert_eq!(Balances::usable_balance(ALICE), prev_balance.saturating_sub(vote_balance));
	});
}
