// This file is part of Substrate.

// Copyright (C) 2020-2025 Acala Foundation.
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

//! This module provides traits for data feeding and provisioning.

use sp_runtime::DispatchResult;
use sp_std::vec::Vec;

/// A trait for feeding data to a data provider.
pub trait DataFeeder<Key, Value, AccountId> {
	/// Feeds a new value for a given key.
	fn feed_value(who: Option<AccountId>, key: Key, value: Value) -> DispatchResult;
}

/// A simple trait for providing data.
pub trait DataProvider<Key, Value> {
	/// Returns the data for a given key.
	fn get(key: &Key) -> Option<Value>;
}

/// An extended `DataProvider` that provides timestamped data.
pub trait DataProviderExtended<Key, TimestampedValue> {
	/// Returns the timestamped value for a given key.
	fn get_no_op(key: &Key) -> Option<TimestampedValue>;
	/// Returns a list of all keys and their optional timestamped values.
	fn get_all_values() -> Vec<(Key, Option<TimestampedValue>)>;
}

#[allow(dead_code)] // rust cannot detect usage in macro_rules
pub fn median<T: Ord + Clone>(mut items: Vec<T>) -> Option<T> {
	if items.is_empty() {
		return None;
	}

	let mid_index = items.len() / 2;

	// Won't panic as `items` ensured not empty.
	let (_, item, _) = items.select_nth_unstable(mid_index);
	Some(item.clone())
}

/// Creates a median data provider from a list of other data providers.
#[macro_export]
macro_rules! create_median_value_data_provider {
	($name:ident, $key:ty, $value:ty, $timestamped_value:ty, [$( $provider:ty ),*]) => {
		pub struct $name;
		impl $crate::DataProvider<$key, $value> for $name {
			fn get(key: &$key) -> Option<$value> {
				let mut values = vec![];
				$(
					if let Some(v) = <$provider as $crate::DataProvider<$key, $value>>::get(&key) {
						values.push(v);
					}
				)*
				$crate::traits::median(values)
			}
		}
		impl $crate::DataProviderExtended<$key, $timestamped_value> for $name {
			fn get_no_op(key: &$key) -> Option<$timestamped_value> {
				let mut values = vec![];
				$(
					if let Some(v) = <$provider as $crate::DataProviderExtended<$key, $timestamped_value>>::get_no_op(&key) {
						values.push(v);
					}
				)*
				$crate::traits::median(values)
			}
			fn get_all_values() -> Vec<($key, Option<$timestamped_value>)> {
				let mut keys = sp_std::collections::btree_set::BTreeSet::new();
				$(
					<$provider as $crate::DataProviderExtended<$key, $timestamped_value>>::get_all_values()
						.into_iter()
						.for_each(|(k, _)| { keys.insert(k); });
				)*
				keys.into_iter().map(|k| (k, Self::get_no_op(&k))).collect()
			}
		}
	}
}

/// Used to combine data from multiple providers.
pub trait CombineData<Key, TimestampedValue> {
	/// Combine data provided by operators
	fn combine_data(
		key: &Key,
		values: Vec<TimestampedValue>,
		prev_value: Option<TimestampedValue>,
	) -> Option<TimestampedValue>;
}

/// A handler for new data events.
#[impl_trait_for_tuples::impl_for_tuples(30)]
pub trait OnNewData<AccountId, Key, Value> {
	/// New data is available
	fn on_new_data(who: &AccountId, key: &Key, value: &Value);
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_std::cell::RefCell;

	thread_local! {
		static MOCK_PRICE_1: RefCell<Option<u8>> = RefCell::new(None);
		static MOCK_PRICE_2: RefCell<Option<u8>> = RefCell::new(None);
		static MOCK_PRICE_3: RefCell<Option<u8>> = RefCell::new(None);
		static MOCK_PRICE_4: RefCell<Option<u8>> = RefCell::new(None);
	}

	macro_rules! mock_data_provider {
		($provider:ident, $price:ident) => {
			pub struct $provider;
			impl $provider {
				fn set_price(price: Option<u8>) {
					$price.with(|v| *v.borrow_mut() = price)
				}
			}
			impl DataProvider<u8, u8> for $provider {
				fn get(_: &u8) -> Option<u8> {
					$price.with(|v| *v.borrow())
				}
			}
			impl DataProviderExtended<u8, u8> for $provider {
				fn get_no_op(_: &u8) -> Option<u8> {
					$price.with(|v| *v.borrow())
				}
				fn get_all_values() -> Vec<(u8, Option<u8>)> {
					vec![(0, Self::get_no_op(&0))]
				}
			}
		};
	}

	mock_data_provider!(Provider1, MOCK_PRICE_1);
	mock_data_provider!(Provider2, MOCK_PRICE_2);
	mock_data_provider!(Provider3, MOCK_PRICE_3);
	mock_data_provider!(Provider4, MOCK_PRICE_4);

	create_median_value_data_provider!(
		Providers,
		u8,
		u8,
		u8,
		[Provider1, Provider2, Provider3, Provider4]
	);

	#[test]
	fn median_value_data_provider_works() {
		assert_eq!(<Providers as DataProvider<_, _>>::get(&0), None);

		let data = vec![
			(vec![None, None, None, Some(1)], Some(1)),
			(vec![None, None, Some(2), Some(1)], Some(2)),
			(vec![Some(5), Some(2), None, Some(7)], Some(5)),
			(vec![Some(5), Some(13), Some(2), Some(7)], Some(7)),
		];

		for (values, target) in data {
			Provider1::set_price(values[0]);
			Provider2::set_price(values[1]);
			Provider3::set_price(values[2]);
			Provider4::set_price(values[3]);

			assert_eq!(<Providers as DataProvider<_, _>>::get(&0), target);
		}
	}
}
