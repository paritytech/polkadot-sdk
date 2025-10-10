// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Tests for Publish XCM instruction.

use super::*;
use crate::test_utils::PublishedData;
use sp_runtime::BoundedVec;
use xcm::latest::{MaxPublishKeyLength, MaxPublishValueLength};

// Helper to create test publish data
fn test_publish_data(items: Vec<(&[u8], &[u8])>) -> PublishData {
	items
		.into_iter()
		.map(|(k, v)| {
			(
				BoundedVec::<u8, MaxPublishKeyLength>::try_from(k.to_vec()).unwrap(),
				BoundedVec::<u8, MaxPublishValueLength>::try_from(v.to_vec()).unwrap(),
			)
		})
		.collect::<Vec<_>>()
		.try_into()
		.unwrap()
}

#[test]
fn publish_from_parachain_works() {
	// Allow unpaid execution from Parachain(1000)
	AllowUnpaidFrom::set(vec![Parachain(1000).into()]);

	let data = test_publish_data(vec![(b"key1", b"value1")]);

	let message = Xcm::<TestCall>(vec![Publish { data: data.clone() }]);
	let mut hash = fake_message_hash(&message);
	let weight_limit = Weight::from_parts(10, 10);

	let r = XcmExecutor::<TestConfig>::prepare_and_execute(
		Parachain(1000),
		message,
		&mut hash,
		weight_limit,
		Weight::zero(),
	);

	assert_eq!(r, Outcome::Complete { used: Weight::from_parts(10, 10) });

	// Verify data was published
	let published = PublishedData::get();
	assert_eq!(published.get(&1000).unwrap().len(), 1);
	assert_eq!(published.get(&1000).unwrap()[0], (b"key1".to_vec(), b"value1".to_vec()));
}

#[test]
fn publish_from_non_parachain_fails() {
	// Allow unpaid execution from Parent to test that origin validation happens
	AllowUnpaidFrom::set(vec![Parent.into()]);

	let data = test_publish_data(vec![(b"key1", b"value1")]);

	let message = Xcm::<TestCall>(vec![Publish { data }]);
	let mut hash = fake_message_hash(&message);
	let weight_limit = Weight::from_parts(10, 10);

	// Try from Parent (not a parachain)
	let r = XcmExecutor::<TestConfig>::prepare_and_execute(
		Parent,
		message,
		&mut hash,
		weight_limit,
		Weight::zero(),
	);

	assert_eq!(
		r,
		Outcome::Incomplete {
			used: Weight::from_parts(10, 10),
			error: InstructionError { index: 0, error: XcmError::BadOrigin },
		}
	);
}

#[test]
fn publish_without_origin_fails() {
	// Allow unpaid execution from Parachain(1000)
	AllowUnpaidFrom::set(vec![Parachain(1000).into()]);

	let data = test_publish_data(vec![(b"key1", b"value1")]);

	let message = Xcm::<TestCall>(vec![ClearOrigin, Publish { data }]);
	let mut hash = fake_message_hash(&message);
	let weight_limit = Weight::from_parts(20, 20);

	let r = XcmExecutor::<TestConfig>::prepare_and_execute(
		Parachain(1000),
		message,
		&mut hash,
		weight_limit,
		Weight::zero(),
	);

	assert_eq!(
		r,
		Outcome::Incomplete {
			used: Weight::from_parts(20, 20),
			error: InstructionError { index: 1, error: XcmError::BadOrigin },
		}
	);
}

#[test]
fn publish_multiple_items_works() {
	// Allow unpaid execution from Parachain(1000)
	AllowUnpaidFrom::set(vec![Parachain(1000).into()]);

	let data = test_publish_data(vec![
		(b"key1", b"value1"),
		(b"key2", b"value2"),
	]);

	let message = Xcm::<TestCall>(vec![Publish { data: data.clone() }]);
	let mut hash = fake_message_hash(&message);
	let weight_limit = Weight::from_parts(10, 10);

	let r = XcmExecutor::<TestConfig>::prepare_and_execute(
		Parachain(1000),
		message,
		&mut hash,
		weight_limit,
		Weight::zero(),
	);

	assert_eq!(r, Outcome::Complete { used: Weight::from_parts(10, 10) });

	// Verify all data was published
	let published = PublishedData::get();
	let para_data = published.get(&1000).unwrap();
	assert_eq!(para_data.len(), 2);
	assert!(para_data.contains(&(b"key1".to_vec(), b"value1".to_vec())));
	assert!(para_data.contains(&(b"key2".to_vec(), b"value2".to_vec())));
}
