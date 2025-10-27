use super::*;
use crate::{
	mock::*,
	IConvictionVoting::{self},
};
use pallet_revive::{
	precompiles::alloy::{
		hex,
		sol_types::{SolInterface, SolValue},
	},
	ExecReturnValue, ExecConfig, Weight, H160, U256,
};

use pallet_conviction_voting::{AccountVote, Conviction, Event, TallyOf, Vote};

fn tally(index: ReferendumIndex) -> TallyOf<Test> {
	<TestPolls as Polling<TallyOf<Test>>>::as_ongoing(index).expect("No poll").0
}

fn class(index: ReferendumIndex) -> TrackId {
	<TestPolls as Polling<TallyOf<Test>>>::as_ongoing(index).expect("No poll").1
}

fn call_precompile(from: AccountId, encoded_call: Vec<u8>) -> Result<ExecReturnValue, sp_runtime::DispatchError> {
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

#[test]
fn test_vote_standard_precompile_works() {
	new_test_ext().execute_with(|| {
		let referendum_index = 3u32;

		let call_params = IConvictionVoting::voteStandardCall {
			referendumIndex: referendum_index,
			aye: true,
			conviction: IConvictionVoting::Conviction::Locked5x,
			balance: 2u128,
		};
		let call = IConvictionVoting::IConvictionVotingCalls::voteStandard(call_params);
		let encoded_call = call.abi_encode();

		assert!(call_precompile(ALICE, encoded_call).is_ok());

		let vote = Vote { aye: true, conviction: Conviction::Locked5x };
		System::assert_last_event(tests::RuntimeEvent::ConvictionVoting(Event::Voted {
			who: ALICE,
			vote: AccountVote::Standard { vote, balance: 2u128 },
			poll_index: referendum_index,
		}));
		assert_eq!(tally(referendum_index), Tally::from_parts(10, 0, 2));
	});
}

#[test]
fn test_vote_split_works() {
	new_test_ext().execute_with(|| {
		let referendum_index = 3u32;

		let call_params = IConvictionVoting::voteSplitCall {
			referendumIndex: referendum_index,
			ayeAmount: 10u128,
			nayAmount: 5u128,
		};
		let call = IConvictionVoting::IConvictionVotingCalls::voteSplit(call_params);
		let encoded_call = call.abi_encode();

		assert!(call_precompile(ALICE, encoded_call).is_ok());

		System::assert_last_event(tests::RuntimeEvent::ConvictionVoting(Event::Voted {
			who: ALICE,
			vote: AccountVote::Split { aye:10u128, nay: 5u128 },
			poll_index: referendum_index,
		}));
		assert_eq!(tally(referendum_index), Tally::from_parts(1, 0, 10));
	});
}

#[test]
fn test_vote_split_abstain_works() {
	new_test_ext().execute_with(|| {
		let referendum_index = 3u32;

		let call_params = IConvictionVoting::voteSplitAbstainCall {
			referendumIndex: referendum_index,
			ayeAmount: 10u128,
			nayAmount: 5u128,
			abstainAmount: 15u128
		};
		let call = IConvictionVoting::IConvictionVotingCalls::voteSplitAbstain(call_params);
		let encoded_call = call.abi_encode();

		assert!(call_precompile(ALICE, encoded_call).is_ok());

		System::assert_last_event(tests::RuntimeEvent::ConvictionVoting(Event::Voted {
			who: ALICE,
			vote: AccountVote::SplitAbstain { aye:10u128, nay: 5u128, abstain: 15u128 },
			poll_index: referendum_index,
		}));
		assert_eq!(tally(referendum_index), Tally::from_parts(1, 0, 25));
	});
}
