use super::*;
use frame_support::{assert_noop, assert_ok};
use mock::*;

#[test]
fn should_feed_values_from_member() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);
		let account_id: AccountId = 1;

		assert_noop!(
			ModuleOracle::feed_values(
				RuntimeOrigin::signed(5),
				vec![(50, 1000), (51, 900), (52, 800)].try_into().unwrap()
			),
			Error::<Test, _>::NoPermission,
		);

		assert_eq!(
			ModuleOracle::feed_values(
				RuntimeOrigin::signed(account_id),
				vec![(50, 1000), (51, 900), (52, 800)].try_into().unwrap()
			)
			.unwrap()
			.pays_fee,
			Pays::No
		);
		System::assert_last_event(RuntimeEvent::ModuleOracle(crate::Event::NewFeedData {
			sender: 1,
			values: vec![(50, 1000), (51, 900), (52, 800)],
		}));

		assert_eq!(
			ModuleOracle::raw_values(&account_id, &50),
			Some(TimestampedValue { value: 1000, timestamp: 12345 })
		);

		assert_eq!(
			ModuleOracle::raw_values(&account_id, &51),
			Some(TimestampedValue { value: 900, timestamp: 12345 })
		);

		assert_eq!(
			ModuleOracle::raw_values(&account_id, &52),
			Some(TimestampedValue { value: 800, timestamp: 12345 })
		);
	});
}

#[test]
fn should_feed_values_from_root() {
	new_test_ext().execute_with(|| {
		let root_feeder: AccountId = RootOperatorAccountId::get();

		assert_ok!(ModuleOracle::feed_values(
			RuntimeOrigin::root(),
			vec![(50, 1000), (51, 900), (52, 800)].try_into().unwrap()
		));

		// Or feed from root using the DataFeeder trait with None
		assert_ok!(ModuleOracle::feed_value(None, 53, 700));

		assert_eq!(
			ModuleOracle::raw_values(&root_feeder, &50),
			Some(TimestampedValue { value: 1000, timestamp: 12345 })
		);

		assert_eq!(
			ModuleOracle::raw_values(&root_feeder, &51),
			Some(TimestampedValue { value: 900, timestamp: 12345 })
		);

		assert_eq!(
			ModuleOracle::raw_values(&root_feeder, &52),
			Some(TimestampedValue { value: 800, timestamp: 12345 })
		);

		assert_eq!(
			ModuleOracle::raw_values(&root_feeder, &53),
			Some(TimestampedValue { value: 700, timestamp: 12345 })
		);
	});
}

#[test]
fn should_not_feed_values_from_root_directly() {
	new_test_ext().execute_with(|| {
		let root_feeder: AccountId = RootOperatorAccountId::get();

		assert_noop!(
			ModuleOracle::feed_values(
				RuntimeOrigin::signed(root_feeder),
				vec![(50, 1000), (51, 900), (52, 800)].try_into().unwrap()
			),
			Error::<Test, _>::NoPermission,
		);
	});
}

#[test]
fn should_read_raw_values() {
	new_test_ext().execute_with(|| {
		let key: u32 = 50;

		let raw_values = ModuleOracle::read_raw_values(&key);
		assert_eq!(raw_values, vec![]);

		assert_ok!(ModuleOracle::feed_values(
			RuntimeOrigin::signed(1),
			vec![(key, 1000)].try_into().unwrap()
		));
		assert_ok!(ModuleOracle::feed_values(
			RuntimeOrigin::signed(2),
			vec![(key, 1200)].try_into().unwrap()
		));

		let raw_values = ModuleOracle::read_raw_values(&key);
		assert_eq!(
			raw_values,
			vec![
				TimestampedValue { value: 1000, timestamp: 12345 },
				TimestampedValue { value: 1200, timestamp: 12345 },
			]
		);
	});
}

#[test]
fn should_combined_data() {
	new_test_ext().execute_with(|| {
		let key: u32 = 50;

		assert_ok!(ModuleOracle::feed_values(
			RuntimeOrigin::signed(1),
			vec![(key, 1300)].try_into().unwrap()
		));
		assert_ok!(ModuleOracle::feed_values(
			RuntimeOrigin::signed(2),
			vec![(key, 1000)].try_into().unwrap()
		));
		assert_ok!(ModuleOracle::feed_values(
			RuntimeOrigin::signed(3),
			vec![(key, 1200)].try_into().unwrap()
		));

		let expected = Some(TimestampedValue { value: 1200, timestamp: 12345 });

		assert_eq!(ModuleOracle::get(&key), expected);

		Timestamp::set_timestamp(23456);

		assert_eq!(ModuleOracle::get(&key), expected);
	});
}

#[test]
fn should_return_none_for_non_exist_key() {
	new_test_ext().execute_with(|| {
		assert_eq!(ModuleOracle::get(&50), None);
	});
}

#[test]
fn multiple_calls_should_fail() {
	new_test_ext().execute_with(|| {
		assert_ok!(ModuleOracle::feed_values(
			RuntimeOrigin::signed(1),
			vec![(50, 1300)].try_into().unwrap()
		));

		// Fails feeding by the extrinsic
		assert_noop!(
			ModuleOracle::feed_values(
				RuntimeOrigin::signed(1),
				vec![(50, 1300)].try_into().unwrap()
			),
			Error::<Test, _>::AlreadyFeeded,
		);

		// But not if fed thought the trait internally
		assert_ok!(ModuleOracle::feed_value(Some(1), 50, 1300));

		ModuleOracle::on_finalize(1);

		assert_ok!(ModuleOracle::feed_values(
			RuntimeOrigin::signed(1),
			vec![(50, 1300)].try_into().unwrap()
		));
	});
}

#[test]
fn get_all_values_should_work() {
	new_test_ext().execute_with(|| {
		let eur: u32 = 1;
		let jpy: u32 = 2;

		assert_eq!(ModuleOracle::get_all_values(), vec![]);

		// feed eur & jpy
		assert_ok!(ModuleOracle::feed_values(
			RuntimeOrigin::signed(1),
			vec![(eur, 1300)].try_into().unwrap()
		));
		assert_ok!(ModuleOracle::feed_values(
			RuntimeOrigin::signed(2),
			vec![(eur, 1000)].try_into().unwrap()
		));
		assert_ok!(ModuleOracle::feed_values(
			RuntimeOrigin::signed(3),
			vec![(jpy, 9000)].try_into().unwrap()
		));

		// not enough eur & jpy prices
		assert_eq!(ModuleOracle::get(&eur), None);
		assert_eq!(ModuleOracle::get(&jpy), None);
		assert_eq!(ModuleOracle::get_all_values(), vec![]);

		// finalize block
		ModuleOracle::on_finalize(1);

		// feed eur & jpy
		assert_ok!(ModuleOracle::feed_values(
			RuntimeOrigin::signed(3),
			vec![(eur, 1200)].try_into().unwrap()
		));
		assert_ok!(ModuleOracle::feed_values(
			RuntimeOrigin::signed(1),
			vec![(jpy, 8000)].try_into().unwrap()
		));

		// enough eur prices
		let eur_price = Some(TimestampedValue { value: 1200, timestamp: 12345 });
		assert_eq!(ModuleOracle::get(&eur), eur_price);

		// not enough jpy prices
		assert_eq!(ModuleOracle::get(&jpy), None);

		assert_eq!(ModuleOracle::get_all_values(), vec![(eur, eur_price)]);

		// feed jpy
		assert_ok!(ModuleOracle::feed_values(
			RuntimeOrigin::signed(2),
			vec![(jpy, 7000)].try_into().unwrap()
		));

		// enough jpy prices
		let jpy_price = Some(TimestampedValue { value: 8000, timestamp: 12345 });
		assert_eq!(ModuleOracle::get(&jpy), jpy_price);

		assert_eq!(ModuleOracle::get_all_values(), vec![(eur, eur_price), (jpy, jpy_price)]);
	});
}

#[test]
fn change_member_should_work() {
	new_test_ext().execute_with(|| {
		set_members(vec![2, 3, 4]);
		<ModuleOracle as ChangeMembers<AccountId>>::change_members_sorted(&[4], &[1], &[2, 3, 4]);
		assert_noop!(
			ModuleOracle::feed_values(
				RuntimeOrigin::signed(1),
				vec![(50, 1000)].try_into().unwrap()
			),
			Error::<Test, _>::NoPermission,
		);
		assert_ok!(ModuleOracle::feed_values(
			RuntimeOrigin::signed(2),
			vec![(50, 1000)].try_into().unwrap()
		));
		assert_ok!(ModuleOracle::feed_values(
			RuntimeOrigin::signed(4),
			vec![(50, 1000)].try_into().unwrap()
		));
	});
}

#[test]
fn should_clear_data_for_removed_members() {
	new_test_ext().execute_with(|| {
		assert_ok!(ModuleOracle::feed_values(
			RuntimeOrigin::signed(1),
			vec![(50, 1000)].try_into().unwrap()
		));
		assert_ok!(ModuleOracle::feed_values(
			RuntimeOrigin::signed(2),
			vec![(50, 1000)].try_into().unwrap()
		));

		ModuleOracle::change_members_sorted(&[4], &[1], &[2, 3, 4]);

		assert_eq!(ModuleOracle::raw_values(&1, 50), None);
	});
}

#[test]
fn values_are_updated_on_feed() {
	new_test_ext().execute_with(|| {
		assert_ok!(ModuleOracle::feed_values(
			RuntimeOrigin::signed(1),
			vec![(50, 900)].try_into().unwrap()
		));
		assert_ok!(ModuleOracle::feed_values(
			RuntimeOrigin::signed(2),
			vec![(50, 1000)].try_into().unwrap()
		));

		assert_eq!(ModuleOracle::values(50), None);

		// Upon the third price feed, the value is updated immediately after `combine`
		// can produce valid result.
		assert_ok!(ModuleOracle::feed_values(
			RuntimeOrigin::signed(3),
			vec![(50, 1100)].try_into().unwrap()
		));
		assert_eq!(
			ModuleOracle::values(50),
			Some(TimestampedValue { value: 1000, timestamp: 12345 })
		);
	});
}
