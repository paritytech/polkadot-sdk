use super::*;
use frame_election_provider_support::SortedListProvider;

#[test]
fn re_nominate_does_not_change_counters_or_list() {
	ExtBuilder::default().nominate(true).build_and_execute(|| {
		// given
		let pre_insert_voter_count =
			(Nominators::<Test>::count() + Validators::<Test>::count()) as u32;
		assert_eq!(<Test as Config>::VoterList::count(), pre_insert_voter_count);

		assert_eq!(<Test as Config>::VoterList::iter().collect::<Vec<_>>(), vec![11, 21, 31, 101]);

		// when account 101 renominates
		assert_ok!(Staking::nominate(RuntimeOrigin::signed(101), vec![41]));

		// then counts don't change
		assert_eq!(<Test as Config>::VoterList::count(), pre_insert_voter_count);
		// and the list is the same
		assert_eq!(<Test as Config>::VoterList::iter().collect::<Vec<_>>(), vec![11, 21, 31, 101]);
	});
}

#[test]
fn re_validate_does_not_change_counters_or_list() {
	ExtBuilder::default().nominate(false).build_and_execute(|| {
		// given
		let pre_insert_voter_count =
			(Nominators::<Test>::count() + Validators::<Test>::count()) as u32;
		assert_eq!(<Test as Config>::VoterList::count(), pre_insert_voter_count);

		assert_eq!(<Test as Config>::VoterList::iter().collect::<Vec<_>>(), vec![11, 21, 31]);

		// when account 11 re-validates
		assert_ok!(Staking::validate(RuntimeOrigin::signed(11), Default::default()));

		// then counts don't change
		assert_eq!(<Test as Config>::VoterList::count(), pre_insert_voter_count);
		// and the list is the same
		assert_eq!(<Test as Config>::VoterList::iter().collect::<Vec<_>>(), vec![11, 21, 31]);
	});
}
