use crate::mock::*;
use sp_runtime::BoundedVec;

use frame_support::traits::fungibles::InspectFreeze;
use pallet_assets::FrozenBalance;

fn basic_freeze() {
	Freezes::<Test>::set(1, 1, BoundedVec::truncate_from(vec![(DummyFreezeReason::Governance, 1)]));
	FrozenBalances::<Test>::insert(1, 1, 1);
}

#[test]
fn it_works_returning_balance_frozen() {
	new_test_ext().execute_with(|| {
		basic_freeze();
		assert_eq!(AssetsFreezer::balance_frozen(1, &DummyFreezeReason::Governance, &1), 1u64);
	});
}

#[test]
fn it_works_returning_frozen_balances() {
	new_test_ext().execute_with(|| {
		basic_freeze();
		assert_eq!(AssetsFreezer::frozen_balance(1, &1), Some(1u64));
		FrozenBalances::<Test>::insert(1, 1, 3);
		assert_eq!(AssetsFreezer::frozen_balance(1, &1), Some(3u64));
	});
}

#[test]
fn it_works_returning_can_freeze() {
	new_test_ext().execute_with(|| {
		basic_freeze();
		assert!(AssetsFreezer::can_freeze(1, &DummyFreezeReason::Staking, &1));
		Freezes::<Test>::mutate(&1, &1, |f| {
			f.try_push((DummyFreezeReason::Staking, 1))
				.expect("current freezes size is less than max freezes; qed");
		});
		assert!(!AssetsFreezer::can_freeze(1, &DummyFreezeReason::Other, &1));
	});
}
