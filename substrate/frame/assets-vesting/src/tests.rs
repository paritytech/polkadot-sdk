use super::{mock::*, AccountIdOf, AssetIdOf, Error, Vesting as VestingStorage, VestingInfo};
use codec::EncodeLike;
use frame::traits::fungibles::VestingSchedule;

const ASSET_ID: AssetId = 1;
const MINIMUM_BALANCE: Balance = 256;

/// Calls vest, and asserts that there is no entry for `account`
/// in the `Vesting` storage item.
fn vest_and_assert_no_vesting<T, I: 'static>(asset: AssetId, account: AccountId)
where
	AssetId: EncodeLike<AssetIdOf<T, I>>,
	AccountId: EncodeLike<AccountIdOf<T>>,
	T: crate::Config<I> + pallet_assets::Config<I>,
{
	// Its ok for this to fail because the user may already have no schedules.
	let _result = AssetsVesting::vest(Some(account).into(), asset.clone());
	assert!(!<VestingStorage<T, I>>::contains_key(asset, account));
}

#[test]
fn check_vesting_status() {
	ExtBuilder::default()
		.with_min_balance(ASSET_ID, MINIMUM_BALANCE)
		.build()
		.execute_with(|| {
			let user_1 = 1;
			let user_2 = 2;
			let user_12 = 12;

			let user1_account_balance = Assets::balance(ASSET_ID, &user_1);
			let user2_account_balance = Assets::balance(ASSET_ID, &user_2);
			let user12_account_balance = Assets::balance(ASSET_ID, &user_12);

			assert_eq!(user1_account_balance, MINIMUM_BALANCE * 10); // Account 1 has balance
			assert_eq!(user2_account_balance, MINIMUM_BALANCE * 20); // Account 2 has balance
			assert_eq!(user12_account_balance, MINIMUM_BALANCE * 10); // Account 12 has balance

			let user1_vesting_schedule = VestingInfo::new(
				MINIMUM_BALANCE * 5,
				128, // Vesting over 10 blocks
				0,
			);
			let user2_vesting_schedule = VestingInfo::new(
				MINIMUM_BALANCE * 20,
				MINIMUM_BALANCE, // Vesting over 20 blocks
				10,
			);
			let user12_vesting_schedule = VestingInfo::new(
				MINIMUM_BALANCE * 5,
				64, // Vesting over 20 blocks
				10,
			);

			assert_eq!(
				VestingStorage::<Test>::get(ASSET_ID, &user_1).unwrap(),
				vec![user1_vesting_schedule]
			); // Account 1 has a vesting schedule
			assert_eq!(
				VestingStorage::<Test>::get(ASSET_ID, &user_2).unwrap(),
				vec![user2_vesting_schedule]
			); // Account 2 has a vesting schedule
			assert_eq!(
				VestingStorage::<Test>::get(ASSET_ID, &user_12).unwrap(),
				vec![user12_vesting_schedule]
			); // Account 12 has a vesting schedule

			// Account 1 has only 128 units vested from their illiquid MINIMUM_BALANCE * 5 units at
			// block 1
			assert_eq!(AssetsVesting::vesting_balance(ASSET_ID, &user_1), Some(128 * 9));
			// Account 2 has their full balance locked
			assert_eq!(
				AssetsVesting::vesting_balance(ASSET_ID, &user_2),
				Some(user2_account_balance)
			);
			// Account 12 has only their illiquid funds locked
			assert_eq!(
				AssetsVesting::vesting_balance(ASSET_ID, &user_12),
				Some(user12_account_balance - MINIMUM_BALANCE * 5)
			);

			System::set_block_number(10);
			assert_eq!(System::block_number(), 10);

			// Account 1 has fully vested by block 10
			assert_eq!(AssetsVesting::vesting_balance(ASSET_ID, &1), Some(0));
			// Account 2 has started vesting by block 10
			assert_eq!(AssetsVesting::vesting_balance(ASSET_ID, &2), Some(user2_account_balance));
			// Account 12 has started vesting by block 10
			assert_eq!(
				AssetsVesting::vesting_balance(ASSET_ID, &12),
				Some(user12_account_balance - MINIMUM_BALANCE * 5)
			);

			System::set_block_number(30);
			assert_eq!(System::block_number(), 30);

			assert_eq!(AssetsVesting::vesting_balance(ASSET_ID, &1), Some(0)); // Account 1 is still fully vested, and not negative
			assert_eq!(AssetsVesting::vesting_balance(ASSET_ID, &2), Some(0)); // Account 2 has fully vested by block 30
			assert_eq!(AssetsVesting::vesting_balance(ASSET_ID, &12), Some(0)); // Account 2 has fully vested by block 30

			// Once we unlock the funds, they are removed from storage.
			vest_and_assert_no_vesting::<Test, ()>(ASSET_ID, 1);
			vest_and_assert_no_vesting::<Test, ()>(ASSET_ID, 2);
			vest_and_assert_no_vesting::<Test, ()>(ASSET_ID, 12);
		});
}

#[test]
fn check_vesting_status_for_multi_schedule_account() {
	ExtBuilder::default()
		.with_min_balance(ASSET_ID, MINIMUM_BALANCE)
		.build()
		.execute_with(|| {
			assert_eq!(System::block_number(), 1);
			let sched0 = VestingInfo::new(
				MINIMUM_BALANCE * 20,
				MINIMUM_BALANCE, // Vesting over 20 blocks
				10,
			);
			// Account 2 already has a vesting schedule.
			assert_eq!(VestingStorage::<Test>::get(ASSET_ID, &2).unwrap(), vec![sched0]);

			// Account 2's free balance is from sched0.
			let account_balance = Assets::balance(ASSET_ID, &2);
			assert_eq!(account_balance, MINIMUM_BALANCE * (20));
			assert_eq!(AssetsVesting::vesting_balance(ASSET_ID, &2), Some(account_balance));

			// Add a 2nd schedule that is already unlocking by block #1.
			let sched1 = VestingInfo::new(
				MINIMUM_BALANCE * 10,
				MINIMUM_BALANCE, // Vesting over 10 blocks
				0,
			);
			assert_ok!(AssetsVesting::vested_transfer(Some(4).into(), ASSET_ID, 2, sched1));
			// Free balance is equal to the two existing schedules total amount.
			let account_balance = Assets::balance(ASSET_ID, &2);
			assert_eq!(account_balance, MINIMUM_BALANCE * (10 + 20));
			// The most recently added schedule exists.
			assert_eq!(VestingStorage::<Test>::get(ASSET_ID, &2).unwrap(), vec![sched0, sched1]);
			// sched1 has free funds at block #1, but nothing else.
			assert_eq!(
				AssetsVesting::vesting_balance(ASSET_ID, &2),
				Some(account_balance - sched1.per_block())
			);

			// Add a 3rd schedule.
			let sched2 = VestingInfo::new(
				MINIMUM_BALANCE * 30,
				MINIMUM_BALANCE, // Vesting over 30 blocks
				5,
			);
			assert_ok!(AssetsVesting::vested_transfer(Some(4).into(), ASSET_ID, 2, sched2));

			System::set_block_number(9);
			// Free balance is equal to the 3 existing schedules total amount.
			let account_balance = Assets::balance(ASSET_ID, &2);
			assert_eq!(account_balance, MINIMUM_BALANCE * (10 + 20 + 30));
			// sched1 and sched2 are freeing funds at block #9.
			assert_eq!(
				AssetsVesting::vesting_balance(ASSET_ID, &2),
				Some(account_balance - sched1.per_block() * 9 - sched2.per_block() * 4)
			);

			System::set_block_number(20);
			// At block #20 sched1 is fully unlocked while sched2 and sched0 are partially unlocked.
			assert_eq!(
				AssetsVesting::vesting_balance(ASSET_ID, &2),
				Some(
					account_balance -
						sched1.locked() - sched2.per_block() * 15 -
						sched0.per_block() * 10
				)
			);

			System::set_block_number(30);
			// At block #30 sched0 and sched1 are fully unlocked while sched2 is partially unlocked.
			assert_eq!(
				AssetsVesting::vesting_balance(ASSET_ID, &2),
				Some(account_balance - sched1.locked() - sched2.per_block() * 25 - sched0.locked())
			);

			// At block #35 sched2 fully unlocks and thus all schedules funds are unlocked.
			System::set_block_number(35);
			assert_eq!(AssetsVesting::vesting_balance(ASSET_ID, &2), Some(0));
			// Since we have not called any extrinsics that would unlock funds the schedules
			// are still in storage,
			assert_eq!(
				VestingStorage::<Test>::get(ASSET_ID, &2).unwrap(),
				vec![sched0, sched1, sched2]
			);
			// but once we unlock the funds, they are removed from storage.
			vest_and_assert_no_vesting::<Test, ()>(ASSET_ID, 2);
		});
}

#[test]
fn unvested_balance_should_not_transfer() {
	ExtBuilder::default().with_min_balance(ASSET_ID, 10).build().execute_with(|| {
		let user1_account_balance = Assets::balance(ASSET_ID, &1);
		assert_eq!(user1_account_balance, 100); // Account 1 has account balance
										  // Account 1 has only 5 units vested at block 1 (plus 50 unvested)
		assert_eq!(AssetsVesting::vesting_balance(ASSET_ID, &1), Some(45));
		// Account 1 cannot send more than vested amount...
		// TODO: this error should change to `TokenError::Frozen` once #4530 is merged
		assert_noop!(
			Assets::transfer(Some(1).into(), ASSET_ID, 2, 56),
			pallet_assets::Error::<Test>::BalanceLow
		);
	});
}

#[test]
fn vested_balance_should_transfer() {
	ExtBuilder::default().with_min_balance(ASSET_ID, 10).build().execute_with(|| {
		let user1_account_balance = Assets::balance(ASSET_ID, &1);
		assert_eq!(user1_account_balance, 100); // Account 1 has account balance
										  // Account 1 has only 5 units vested at block 1 (plus 50 unvested)
		assert_eq!(AssetsVesting::vesting_balance(ASSET_ID, &1), Some(45));
		assert_ok!(AssetsVesting::vest(Some(1).into(), ASSET_ID));
		// TODO: this value should be changed to 55 once #4530 is merged
		assert_ok!(Assets::transfer(Some(1).into(), ASSET_ID, 2, 45));
	});
}

#[test]
fn vested_balance_should_transfer_with_multi_sched() {
	ExtBuilder::default()
		.with_min_balance(ASSET_ID, MINIMUM_BALANCE)
		.build()
		.execute_with(|| {
			let sched0 = VestingInfo::new(5 * MINIMUM_BALANCE, 128, 0);
			assert_ok!(AssetsVesting::vested_transfer(Some(13).into(), ASSET_ID, 1, sched0));
			// Total 10*ED locked for all the schedules.
			assert_eq!(VestingStorage::<Test>::get(ASSET_ID, &1).unwrap(), vec![sched0, sched0]);

			let user1_account_balance = Assets::balance(ASSET_ID, &1);
			assert_eq!(user1_account_balance, 3840); // Account 1 has account balance

			// Account 1 has only 256 units unlocking at block 1 (plus 1280 already fee).
			assert_eq!(AssetsVesting::vesting_balance(ASSET_ID, &1), Some(2304));
			assert_ok!(AssetsVesting::vest(Some(1).into(), ASSET_ID));
			// TODO: this value should be changed to 1536 once #4530 is merged
			assert_ok!(Assets::transfer(Some(1).into(), ASSET_ID, 2, 1280));
		});
}

#[test]
fn non_vested_cannot_vest() {
	ExtBuilder::default()
		.with_min_balance(ASSET_ID, MINIMUM_BALANCE)
		.build()
		.execute_with(|| {
			assert!(!<VestingStorage<Test>>::contains_key(ASSET_ID, 4));
			assert_noop!(AssetsVesting::vest(Some(4).into(), ASSET_ID), Error::<Test>::NotVesting);
		});
}

#[test]
fn vested_balance_should_transfer_using_vest_other() {
	ExtBuilder::default().with_min_balance(ASSET_ID, 10).build().execute_with(|| {
		let user1_account_balance = Assets::balance(ASSET_ID, &1);
		assert_eq!(user1_account_balance, 100); // Account 1 has account balance
										  // Account 1 has only 5 units vested at block 1 (plus 50 unvested)
		assert_eq!(AssetsVesting::vesting_balance(ASSET_ID, &1), Some(45));
		assert_ok!(AssetsVesting::vest_other(Some(2).into(), ASSET_ID, 1));
		// TODO: this value should be changed to 55 once #4530 is merged
		assert_ok!(Assets::transfer(Some(1).into(), ASSET_ID, 2, 45));
	});
}

#[test]
fn vested_balance_should_transfer_using_vest_other_with_multi_sched() {
	ExtBuilder::default()
		.with_min_balance(ASSET_ID, MINIMUM_BALANCE)
		.build()
		.execute_with(|| {
			let sched0 = VestingInfo::new(5 * MINIMUM_BALANCE, 128, 0);
			assert_ok!(AssetsVesting::vested_transfer(Some(13).into(), ASSET_ID, 1, sched0));
			// Total of 10*ED of locked for all the schedules.
			assert_eq!(VestingStorage::<Test>::get(ASSET_ID, &1).unwrap(), vec![sched0, sched0]);

			let user1_account_balance = Assets::balance(ASSET_ID, &1);
			assert_eq!(user1_account_balance, 3840); // Account 1 has account balance

			// Account 1 has only 256 units unlocking at block 1 (plus 1280 already free).
			assert_eq!(AssetsVesting::vesting_balance(ASSET_ID, &1), Some(2304));
			assert_ok!(AssetsVesting::vest_other(Some(2).into(), ASSET_ID, 1));
			// TODO: this value should be changed to 1536 once #4530 is merged
			assert_ok!(Assets::transfer(Some(1).into(), ASSET_ID, 2, 1280));
		});
}

#[test]
fn non_vested_cannot_vest_other() {
	ExtBuilder::default()
		.with_min_balance(ASSET_ID, MINIMUM_BALANCE)
		.build()
		.execute_with(|| {
			assert!(!<VestingStorage<Test>>::contains_key(ASSET_ID, 4));
			assert_noop!(
				AssetsVesting::vest_other(Some(3).into(), ASSET_ID, 4),
				Error::<Test>::NotVesting
			);
		});
}
