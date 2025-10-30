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

//! Adapters for broadcast/publish operations in XCM.

use alloc::vec::Vec;
use core::marker::PhantomData;
use frame_support::traits::Contains;
use polkadot_primitives::Id as ParaId;
use polkadot_runtime_parachains::broadcaster::PublishSubscribe;
use xcm::latest::prelude::XcmError;
use xcm::latest::{Junction, Location, PublishData, Result as XcmResult};
use xcm_executor::traits::BroadcastHandler;

/// Configurable broadcast adapter that validates parachain origins.
pub struct ParachainBroadcastAdapter<Filter, Handler>(PhantomData<(Filter, Handler)>);

impl<Filter, Handler> BroadcastHandler for ParachainBroadcastAdapter<Filter, Handler>
where
	Filter: Contains<Location>,
	Handler: PublishSubscribe,
{
	fn handle_publish(origin: &Location, data: PublishData) -> XcmResult {
		// Check if origin is authorized to publish
		if !Filter::contains(origin) {
			return Err(XcmError::NoPermission);
		}

		// Extract parachain ID from authorized origin
		let para_id = match origin.unpack() {
			(0, [Junction::Parachain(id)]) => ParaId::from(*id), // Direct parachain
			(1, [Junction::Parachain(id), ..]) => ParaId::from(*id), // Sibling parachain
			_ => return Err(XcmError::BadOrigin),                // Should be caught by filter
		};

		// Call the actual handler
		let data_vec: Vec<(Vec<u8>, Vec<u8>)> = data
			.into_inner()
			.into_iter()
			.map(|(k, v)| (k.into_inner(), v.into_inner()))
			.collect();
		Handler::publish_data(para_id, data_vec).map_err(|_| XcmError::PublishFailed)
	}

	fn handle_subscribe(origin: &Location, publisher: u32) -> XcmResult {
		// Check if origin is authorized to subscribe
		if !Filter::contains(origin) {
			return Err(XcmError::NoPermission);
		}

		// Extract subscriber parachain ID from authorized origin
		let subscriber_id = match origin.unpack() {
			(0, [Junction::Parachain(id)]) => ParaId::from(*id), // Direct parachain
			(1, [Junction::Parachain(id), ..]) => ParaId::from(*id), // Sibling parachain
			_ => return Err(XcmError::BadOrigin),                // Should be caught by filter
		};

		let publisher_id = ParaId::from(publisher);

		// Call the handler for subscribe toggle
		Handler::toggle_subscription(subscriber_id, publisher_id)
			.map_err(|_| XcmError::SubscribeFailed)
	}
}

/// Allows only direct parachains (parents=0, interior=[Parachain(_)]).
pub struct OnlyParachains;
impl Contains<Location> for OnlyParachains {
	fn contains(origin: &Location) -> bool {
		matches!(origin.unpack(), (0, [Junction::Parachain(_)]))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use frame_support::parameter_types;
	use polkadot_runtime_parachains::broadcaster::PublishSubscribe;
	use sp_runtime::BoundedVec;
	use xcm::latest::prelude::XcmError;
	use xcm::latest::{
		Junction, Location, MaxPublishKeyLength, MaxPublishValueLength, PublishData,
	};

	// Mock handler that tracks calls
	parameter_types! {
		pub static PublishCalls: Vec<(ParaId, Vec<(Vec<u8>, Vec<u8>)>)> = vec![];
		pub static SubscribeCalls: Vec<(ParaId, ParaId)> = vec![];
	}

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

	struct MockPublishHandler;
	impl PublishSubscribe for MockPublishHandler {
		fn publish_data(
			publisher: ParaId,
			data: Vec<(Vec<u8>, Vec<u8>)>,
		) -> Result<(), sp_runtime::DispatchError> {
			let mut calls = PublishCalls::get();
			calls.push((publisher, data));
			PublishCalls::set(calls);
			Ok(())
		}

		fn toggle_subscription(
			subscriber: ParaId,
			publisher: ParaId,
		) -> Result<(), sp_runtime::DispatchError> {
			let mut calls = SubscribeCalls::get();
			calls.push((subscriber, publisher));
			SubscribeCalls::set(calls);
			Ok(())
		}
	}

	#[test]
	fn publish_from_direct_parachain_works() {
		PublishCalls::set(vec![]);
		let origin = Location::new(0, [Junction::Parachain(1000)]);
		let data = test_publish_data(vec![(b"key1", b"value1")]);

		let result = ParachainBroadcastAdapter::<OnlyParachains, MockPublishHandler>::handle_publish(
			&origin,
			data.clone(),
		);

		assert!(result.is_ok());
		let calls = PublishCalls::get();
		assert_eq!(calls.len(), 1);
		assert_eq!(calls[0].0, ParaId::from(1000));
		assert_eq!(calls[0].1, vec![(b"key1".to_vec(), b"value1".to_vec())]);
	}

	#[test]
	fn publish_from_sibling_parachain_fails() {
		PublishCalls::set(vec![]);
		let origin = Location::new(
			1,
			[Junction::Parachain(2000), Junction::AccountId32 { network: None, id: [1; 32] }],
		);
		let data = test_publish_data(vec![(b"key1", b"value1")]);

		let result = ParachainBroadcastAdapter::<OnlyParachains, MockPublishHandler>::handle_publish(
			&origin,
			data.clone(),
		);

		assert!(matches!(result, Err(XcmError::NoPermission)));
		assert!(PublishCalls::get().is_empty());
	}

	#[test]
	fn publish_from_non_parachain_fails() {
		PublishCalls::set(vec![]);
		let origin = Location::here();
		let data = test_publish_data(vec![(b"key1", b"value1")]);

		let result =
			ParachainBroadcastAdapter::<OnlyParachains, MockPublishHandler>::handle_publish(
				&origin, data,
			);

		assert!(matches!(result, Err(XcmError::NoPermission)));
		assert!(PublishCalls::get().is_empty());
	}

	#[test]
	fn only_parachains_filter_works() {
		// Direct parachain allowed
		assert!(OnlyParachains::contains(&Location::new(0, [Junction::Parachain(1000)])));

		// Sibling parachain not allowed
		assert!(!OnlyParachains::contains(&Location::new(1, [Junction::Parachain(1000)])));

		// Root not allowed
		assert!(!OnlyParachains::contains(&Location::here()));
	}

	#[test]
	fn subscribe_from_direct_parachain_works() {
		SubscribeCalls::set(vec![]);
		let origin = Location::new(0, [Junction::Parachain(1000)]);
		let publisher = 2000;

		let result =
			ParachainBroadcastAdapter::<OnlyParachains, MockPublishHandler>::handle_subscribe(
				&origin, publisher,
			);

		assert!(result.is_ok());
		let calls = SubscribeCalls::get();
		assert_eq!(calls.len(), 1);
		assert_eq!(calls[0], (ParaId::from(1000), ParaId::from(2000)));
	}

	#[test]
	fn subscribe_from_sibling_parachain_fails() {
		SubscribeCalls::set(vec![]);
		let origin = Location::new(
			1,
			[Junction::Parachain(3000), Junction::AccountId32 { network: None, id: [1; 32] }],
		);
		let publisher = 2000;

		let result =
			ParachainBroadcastAdapter::<OnlyParachains, MockPublishHandler>::handle_subscribe(
				&origin, publisher,
			);

		assert!(matches!(result, Err(XcmError::NoPermission)));
		assert!(SubscribeCalls::get().is_empty());
	}

	#[test]
	fn subscribe_from_non_parachain_fails() {
		SubscribeCalls::set(vec![]);
		let origin = Location::here();
		let publisher = 2000;

		let result =
			ParachainBroadcastAdapter::<OnlyParachains, MockPublishHandler>::handle_subscribe(
				&origin, publisher,
			);

		assert!(matches!(result, Err(XcmError::NoPermission)));
		assert!(SubscribeCalls::get().is_empty());
	}
}
