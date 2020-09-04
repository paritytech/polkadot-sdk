// Copyright 2019-2020 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Runtime module which takes care of dispatching messages received over the bridge.
//!
//! The messages are interpreted directly as runtime `Call`s, we attempt to decode
//! them and then dispatch as usualy.
//! To prevent compatibility issues, the calls have to include `spec_version` as well
//! which is being checked before dispatch.
//!
//! In case of succesful dispatch event is emitted.

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]

use bp_message_dispatch::{MessageDispatch, Weight};
use bp_runtime::{bridge_account_id, InstanceId, CALL_DISPATCH_MODULE_PREFIX};
use frame_support::{
	decl_event, decl_module,
	dispatch::{Dispatchable, Parameter},
	traits::Get,
	weights::{extract_actual_weight, GetDispatchInfo},
};
use sp_runtime::DispatchResult;

/// Spec version type.
pub type SpecVersion = u32;

// TODO: update me (https://github.com/paritytech/parity-bridges-common/issues/78)
/// Weight of single deposit_event() call.
const DEPOSIT_EVENT_WEIGHT: Weight = 0;

/// The module configuration trait.
pub trait Trait: frame_system::Trait {
	/// The overarching event type.
	type Event: From<Event<Self>> + Into<<Self as frame_system::Trait>::Event>;
	/// Id of the message. Whenever message is passed to the dispatch module, it emits
	/// event with this id + dispatch result. Could be e.g. (LaneId, MessageNonce) if
	/// it comes from message-lane module.
	type MessageId: Parameter;
	/// The overarching dispatch call type.
	type Call: Parameter
		+ GetDispatchInfo
		+ Dispatchable<
			Origin = <Self as frame_system::Trait>::Origin,
			PostInfo = frame_support::dispatch::PostDispatchInfo,
		>;
}

decl_event!(
	pub enum Event<T> where
		<T as Trait>::MessageId,
	{
		/// Message has been rejected by dispatcher because of spec version mismatch.
		/// Last two arguments are: expected and passed spec version.
		MessageVersionSpecMismatch(InstanceId, MessageId, SpecVersion, SpecVersion),
		/// Message has been rejected by dispatcher because of weight mismatch.
		/// Last two arguments are: expected and passed call weight.
		MessageWeightMismatch(InstanceId, MessageId, Weight, Weight),
		/// Message has been dispatched with given result.
		MessageDispatched(InstanceId, MessageId, DispatchResult),
	}
);

decl_module! {
	/// Call Dispatch FRAME Pallet.
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		/// Deposit one of this module's events by using the default implementation.
		fn deposit_event() = default;
	}
}

impl<T: Trait> MessageDispatch<T::MessageId> for Module<T> {
	type Message = (SpecVersion, Weight, <T as Trait>::Call);

	fn dispatch(bridge: InstanceId, id: T::MessageId, message: Self::Message) -> Weight {
		let (spec_version, weight, call) = message;

		// verify spec version
		// (we want it to be the same, because otherwise we may decode Call improperly)
		let expected_version = <T as frame_system::Trait>::Version::get().spec_version;
		if spec_version != expected_version {
			frame_support::debug::trace!(
				"Message {:?}/{:?}: spec_version mismatch. Expected {:?}, got {:?}",
				bridge,
				id,
				expected_version,
				spec_version,
			);
			Self::deposit_event(Event::<T>::MessageVersionSpecMismatch(
				bridge,
				id,
				expected_version,
				spec_version,
			));
			return DEPOSIT_EVENT_WEIGHT;
		}

		// verify weight
		// (we want passed weight to be at least equal to pre-dispatch weight of the call
		// because otherwise Calls may be dispatched at lower price)
		let dispatch_info = call.get_dispatch_info();
		let expected_weight = dispatch_info.weight;
		if weight < expected_weight {
			frame_support::debug::trace!(
				"Message {:?}/{:?}: passed weight is too low. Expected at least {:?}, got {:?}",
				bridge,
				id,
				expected_weight,
				weight,
			);
			Self::deposit_event(Event::<T>::MessageWeightMismatch(bridge, id, expected_weight, weight));
			return DEPOSIT_EVENT_WEIGHT;
		}

		// finally dispatch message
		let origin_account = bridge_account_id(bridge, CALL_DISPATCH_MODULE_PREFIX);
		let dispatch_result = call.dispatch(frame_system::RawOrigin::Signed(origin_account).into());
		let actual_call_weight = extract_actual_weight(&dispatch_result, &dispatch_info);
		frame_support::debug::trace!(
			"Message {:?}/{:?} has been dispatched. Result: {:?}",
			bridge,
			id,
			dispatch_result,
		);

		Self::deposit_event(Event::<T>::MessageDispatched(
			bridge,
			id,
			dispatch_result.map(drop).map_err(|e| e.error),
		));

		actual_call_weight + DEPOSIT_EVENT_WEIGHT
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use frame_support::{impl_outer_dispatch, impl_outer_event, impl_outer_origin, parameter_types, weights::Weight};
	use frame_system::{EventRecord, Phase};
	use sp_core::H256;
	use sp_runtime::{
		testing::Header,
		traits::{BlakeTwo256, IdentityLookup},
		DispatchError, Perbill,
	};

	type AccountId = u64;
	type CallDispatch = Module<TestRuntime>;
	type System = frame_system::Module<TestRuntime>;

	type MessageId = [u8; 4];

	#[derive(Clone, Eq, PartialEq)]
	pub struct TestRuntime;

	mod call_dispatch {
		pub use crate::Event;
	}

	impl_outer_event! {
		pub enum TestEvent for TestRuntime {
			frame_system<T>,
			call_dispatch<T>,
		}
	}

	impl_outer_origin! {
		pub enum Origin for TestRuntime where system = frame_system {}
	}

	impl_outer_dispatch! {
		pub enum Call for TestRuntime where origin: Origin {
			frame_system::System,
			call_dispatch::CallDispatch,
		}
	}

	parameter_types! {
		pub const BlockHashCount: u64 = 250;
		pub const MaximumBlockWeight: Weight = 1024;
		pub const MaximumBlockLength: u32 = 2 * 1024;
		pub const AvailableBlockRatio: Perbill = Perbill::one();
	}

	impl frame_system::Trait for TestRuntime {
		type Origin = Origin;
		type Index = u64;
		type Call = Call;
		type BlockNumber = u64;
		type Hash = H256;
		type Hashing = BlakeTwo256;
		type AccountId = AccountId;
		type Lookup = IdentityLookup<Self::AccountId>;
		type Header = Header;
		type Event = TestEvent;
		type BlockHashCount = BlockHashCount;
		type MaximumBlockWeight = MaximumBlockWeight;
		type DbWeight = ();
		type BlockExecutionWeight = ();
		type ExtrinsicBaseWeight = ();
		type MaximumExtrinsicWeight = ();
		type AvailableBlockRatio = AvailableBlockRatio;
		type MaximumBlockLength = MaximumBlockLength;
		type Version = ();
		type ModuleToIndex = ();
		type AccountData = ();
		type OnNewAccount = ();
		type OnKilledAccount = ();
		type BaseCallFilter = ();
		type SystemWeightInfo = ();
	}

	impl Trait for TestRuntime {
		type Event = TestEvent;
		type MessageId = MessageId;
		type Call = Call;
	}

	fn new_test_ext() -> sp_io::TestExternalities {
		let t = frame_system::GenesisConfig::default()
			.build_storage::<TestRuntime>()
			.unwrap();
		sp_io::TestExternalities::new(t)
	}

	#[test]
	fn should_succesfuly_dispatch_remark() {
		new_test_ext().execute_with(|| {
			let origin = b"ethb".to_owned();
			let id = [0; 4];
			let message = (
				0,
				1_000_000_000,
				Call::System(<frame_system::Call<TestRuntime>>::remark(vec![1, 2, 3])),
			);

			System::set_block_number(1);
			CallDispatch::dispatch(origin, id, message);

			assert_eq!(
				System::events(),
				vec![EventRecord {
					phase: Phase::Initialization,
					event: TestEvent::call_dispatch(Event::<TestRuntime>::MessageDispatched(origin, id, Ok(()))),
					topics: vec![],
				}],
			);
		});
	}

	#[test]
	fn should_fail_on_spec_version_mismatch() {
		new_test_ext().execute_with(|| {
			let origin = b"ethb".to_owned();
			let id = [0; 4];
			let message = (
				69,
				1_000_000,
				Call::System(<frame_system::Call<TestRuntime>>::remark(vec![1, 2, 3])),
			);

			System::set_block_number(1);
			CallDispatch::dispatch(origin, id, message);

			assert_eq!(
				System::events(),
				vec![EventRecord {
					phase: Phase::Initialization,
					event: TestEvent::call_dispatch(Event::<TestRuntime>::MessageVersionSpecMismatch(
						origin, id, 0, 69,
					)),
					topics: vec![],
				}],
			);
		});
	}

	#[test]
	fn should_fail_on_weight_mismatch() {
		new_test_ext().execute_with(|| {
			let origin = b"ethb".to_owned();
			let id = [0; 4];
			let message = (
				0,
				0,
				Call::System(<frame_system::Call<TestRuntime>>::remark(vec![1, 2, 3])),
			);

			System::set_block_number(1);
			CallDispatch::dispatch(origin, id, message);

			assert_eq!(
				System::events(),
				vec![EventRecord {
					phase: Phase::Initialization,
					event: TestEvent::call_dispatch(Event::<TestRuntime>::MessageWeightMismatch(
						origin, id, 1305000, 0,
					)),
					topics: vec![],
				}],
			);
		});
	}

	#[test]
	fn should_dispatch_from_non_root_origin() {
		new_test_ext().execute_with(|| {
			let origin = b"ethb".to_owned();
			let id = [0; 4];
			let message = (
				0,
				1_000_000,
				Call::System(<frame_system::Call<TestRuntime>>::fill_block(Perbill::from_percent(10))),
			);

			System::set_block_number(1);
			CallDispatch::dispatch(origin, id, message);

			assert_eq!(
				System::events(),
				vec![EventRecord {
					phase: Phase::Initialization,
					event: TestEvent::call_dispatch(Event::<TestRuntime>::MessageDispatched(
						origin,
						id,
						Err(DispatchError::BadOrigin)
					)),
					topics: vec![],
				}],
			);
		});
	}
}
