#![cfg(test)]

use crate::macros::experimental::{hypothetically, hypothetically_ok};
use frame_support::StorageNoopGuard;
use sp_io::{storage::get, TestExternalities as Ext};

#[test]
fn hypothetically_rolls_back_ok() {
	Ext::new(Default::default()).execute_with(|| {
		let _g = StorageNoopGuard::new();

		let res = hypothetically!(modify_ok());
		assert!(res.is_ok(), "Result carries over");
	});
}

#[test]
fn hypothetically_rolls_back_err() {
	Ext::new(Default::default()).execute_with(|| {
		let _g = StorageNoopGuard::new();

		let res = hypothetically!(modify_err());
		assert!(res.is_err(), "Result carries over");
	});
}

#[test]
fn hypothetically_custom_return_value() {
	Ext::new(Default::default()).execute_with(|| {
		assert_eq!(hypothetically!((1, 2, 3)), (1, 2, 3), "Result carries over");
	});
}

#[test]
fn hypothetically_ok_rollback_on_success() {
	Ext::new(Default::default()).execute_with(|| {
		let _g = StorageNoopGuard::new();

		hypothetically_ok!(modify_ok());
	});
}

#[test]
fn hypothetically_ok_rollback_on_err() {
	Ext::new(Default::default()).execute_with(|| {
		let _g = StorageNoopGuard::new();

		std::panic::catch_unwind(|| {
			hypothetically_ok!(modify_err());
		})
		.expect_err("should panic");
	});
}

#[test]
fn hypothetically_ok_explicit_result() {
	Ext::new(Default::default()).execute_with(|| {
		let _g = StorageNoopGuard::new();
		// Test that the second argument is passed into `assert_ok`.
		hypothetically_ok!(modify_ok(), ());
	});
}

const KEY: &[u8] = b"key";

fn modify_ok() -> Result<(), ()> {
	sp_io::storage::set(KEY, b"value");
	assert!(get(KEY).is_some());
	Ok(())
}

fn modify_err() -> Result<(), ()> {
	modify_ok().unwrap();
	assert!(get(KEY).is_some());
	Err(())
}
