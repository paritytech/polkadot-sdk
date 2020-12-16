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
//! The messages are interpreted directly as runtime `Call`. We attempt to decode
//! them and then dispatch as usual. To prevent compatibility issues, the Calls have
//! to include a `spec_version`. This will be checked before dispatch. In the case of
//! a succesful dispatch an event is emitted.

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]

use bp_message_dispatch::{MessageDispatch, Weight};
use bp_runtime::{derive_account_id, InstanceId, SourceAccount};
use codec::{Decode, Encode};
use frame_support::{
	decl_event, decl_module, decl_storage,
	dispatch::{Dispatchable, Parameter},
	ensure,
	traits::Get,
	weights::{extract_actual_weight, GetDispatchInfo},
	RuntimeDebug,
};
use frame_system::RawOrigin;
use sp_runtime::{
	traits::{BadOrigin, Convert, IdentifyAccount, MaybeDisplay, MaybeSerializeDeserialize, Member, Verify},
	DispatchResult,
};
use sp_std::{fmt::Debug, marker::PhantomData, prelude::*};

/// Spec version type.
pub type SpecVersion = u32;

/// Origin of a Call when it is dispatched on the target chain.
///
/// The source chain can (and should) verify that the message can be dispatched on the target chain
/// with a particular origin given the source chain's origin. This can be done with the
/// `verify_message_origin()` function.
#[derive(RuntimeDebug, Encode, Decode, Clone, PartialEq, Eq)]
pub enum CallOrigin<SourceChainAccountId, TargetChainAccountPublic, TargetChainSignature> {
	/// Call is sent by the Root origin on the source chain. On the target chain it is dispatched
	/// from a derived account.
	///
	/// The derived account represents the source Root account on the target chain. This is useful
	/// if the target chain needs some way of knowing that a call came from a priviledged origin on
	/// the source chain (maybe to allow a configuration change for example).
	SourceRoot,

	/// Call is sent by `SourceChainAccountId` on the source chain. On the target chain it is
	/// dispatched from an account controlled by a private key on the target chain.
	///
	/// The account can be identified by `TargetChainAccountPublic`. The proof that the
	/// `SourceChainAccountId` controls `TargetChainAccountPublic` is the `TargetChainSignature`
	/// over `(Call, SourceChainAccountId).encode()`.
	TargetAccount(SourceChainAccountId, TargetChainAccountPublic, TargetChainSignature),

	/// Call is sent by the `SourceChainAccountId` on the source chain. On the target chain it is
	/// dispatched from a derived account ID.
	///
	/// The account ID on the target chain is derived from the source account ID This is useful if
	/// you need a way to represent foreign accounts on this chain for call dispatch purposes.
	///
	/// Note that the derived account does not need to have a private key on the target chain. This
	/// origin can therefore represent proxies, pallets, etc. as well as "regular" accounts.
	SourceAccount(SourceChainAccountId),
}

/// Message payload type used by call-dispatch module.
#[derive(RuntimeDebug, Encode, Decode, Clone, PartialEq, Eq)]
pub struct MessagePayload<SourceChainAccountId, TargetChainAccountPublic, TargetChainSignature, Call> {
	/// Runtime specification version. We only dispatch messages that have the same
	/// runtime version. Otherwise we risk to misinterpret encoded calls.
	pub spec_version: SpecVersion,
	/// Weight of the call, declared by the message sender. If it is less than actual
	/// static weight, the call is not dispatched.
	pub weight: Weight,
	/// Call origin to be used during dispatch.
	pub origin: CallOrigin<SourceChainAccountId, TargetChainAccountPublic, TargetChainSignature>,
	/// The call itself.
	pub call: Call,
}

/// The module configuration trait.
pub trait Config<I = DefaultInstance>: frame_system::Config {
	/// The overarching event type.
	type Event: From<Event<Self, I>> + Into<<Self as frame_system::Config>::Event>;
	/// Id of the message. Whenever message is passed to the dispatch module, it emits
	/// event with this id + dispatch result. Could be e.g. (LaneId, MessageNonce) if
	/// it comes from message-lane module.
	type MessageId: Parameter;
	/// Type of account ID on source chain.
	type SourceChainAccountId: Parameter + Member + MaybeSerializeDeserialize + Debug + MaybeDisplay + Ord + Default;
	/// Type of account public key on target chain.
	type TargetChainAccountPublic: Parameter + IdentifyAccount<AccountId = Self::AccountId>;
	/// Type of signature that may prove that the message has been signed by
	/// owner of `TargetChainAccountPublic`.
	type TargetChainSignature: Parameter + Verify<Signer = Self::TargetChainAccountPublic>;
	/// The overarching dispatch call type.
	type Call: Parameter
		+ GetDispatchInfo
		+ Dispatchable<
			Origin = <Self as frame_system::Config>::Origin,
			PostInfo = frame_support::dispatch::PostDispatchInfo,
		>;
	/// A type which can be turned into an AccountId from a 256-bit hash.
	///
	/// Used when deriving target chain AccountIds from source chain AccountIds.
	type AccountIdConverter: sp_runtime::traits::Convert<sp_core::hash::H256, Self::AccountId>;
}

decl_storage! {
	trait Store for Module<T: Config<I>, I: Instance = DefaultInstance> as CallDispatch {}
}

decl_event!(
	pub enum Event<T, I = DefaultInstance> where
		<T as Config<I>>::MessageId
	{
		/// Message has been rejected by dispatcher because of spec version mismatch.
		/// Last two arguments are: expected and passed spec version.
		MessageVersionSpecMismatch(InstanceId, MessageId, SpecVersion, SpecVersion),
		/// Message has been rejected by dispatcher because of weight mismatch.
		/// Last two arguments are: expected and passed call weight.
		MessageWeightMismatch(InstanceId, MessageId, Weight, Weight),
		/// Message signature mismatch.
		MessageSignatureMismatch(InstanceId, MessageId),
		/// Message has been dispatched with given result.
		MessageDispatched(InstanceId, MessageId, DispatchResult),
		/// Phantom member, never used. Needed to handle multiple pallet instances.
		_Dummy(PhantomData<I>),
	}
);

decl_module! {
	/// Call Dispatch FRAME Pallet.
	pub struct Module<T: Config<I>, I: Instance = DefaultInstance> for enum Call where origin: T::Origin {
		/// Deposit one of this module's events by using the default implementation.
		fn deposit_event() = default;
	}
}

impl<T: Config<I>, I: Instance> MessageDispatch<T::MessageId> for Module<T, I> {
	type Message = MessagePayload<
		T::SourceChainAccountId,
		T::TargetChainAccountPublic,
		T::TargetChainSignature,
		<T as Config<I>>::Call,
	>;

	fn dispatch_weight(message: &Self::Message) -> Weight {
		message.weight
	}

	fn dispatch(bridge: InstanceId, id: T::MessageId, message: Self::Message) {
		// verify spec version
		// (we want it to be the same, because otherwise we may decode Call improperly)
		let expected_version = <T as frame_system::Config>::Version::get().spec_version;
		if message.spec_version != expected_version {
			frame_support::debug::trace!(
				"Message {:?}/{:?}: spec_version mismatch. Expected {:?}, got {:?}",
				bridge,
				id,
				expected_version,
				message.spec_version,
			);
			Self::deposit_event(RawEvent::MessageVersionSpecMismatch(
				bridge,
				id,
				expected_version,
				message.spec_version,
			));
			return;
		}

		// verify weight
		// (we want passed weight to be at least equal to pre-dispatch weight of the call
		// because otherwise Calls may be dispatched at lower price)
		let dispatch_info = message.call.get_dispatch_info();
		let expected_weight = dispatch_info.weight;
		if message.weight < expected_weight {
			frame_support::debug::trace!(
				"Message {:?}/{:?}: passed weight is too low. Expected at least {:?}, got {:?}",
				bridge,
				id,
				expected_weight,
				message.weight,
			);
			Self::deposit_event(RawEvent::MessageWeightMismatch(
				bridge,
				id,
				expected_weight,
				message.weight,
			));
			return;
		}

		// prepare dispatch origin
		let origin_account = match message.origin {
			CallOrigin::SourceRoot => {
				let hex_id = derive_account_id::<T::SourceChainAccountId>(bridge, SourceAccount::Root);
				let target_id = T::AccountIdConverter::convert(hex_id);
				frame_support::debug::trace!("Root Account: {:?}", &target_id);
				target_id
			}
			CallOrigin::TargetAccount(source_account_id, target_public, target_signature) => {
				let mut signed_message = Vec::new();
				message.call.encode_to(&mut signed_message);
				source_account_id.encode_to(&mut signed_message);

				let target_account = target_public.into_account();
				if !target_signature.verify(&signed_message[..], &target_account) {
					frame_support::debug::trace!(
						"Message {:?}/{:?}: origin proof is invalid. Expected account: {:?} from signature: {:?}",
						bridge,
						id,
						target_account,
						target_signature,
					);
					Self::deposit_event(RawEvent::MessageSignatureMismatch(bridge, id));
					return;
				}

				frame_support::debug::trace!("Target Account: {:?}", &target_account);
				target_account
			}
			CallOrigin::SourceAccount(source_account_id) => {
				let hex_id = derive_account_id(bridge, SourceAccount::Account(source_account_id));
				let target_id = T::AccountIdConverter::convert(hex_id);
				frame_support::debug::trace!("Source Account: {:?}", &target_id);
				target_id
			}
		};

		// finally dispatch message
		let origin = RawOrigin::Signed(origin_account).into();

		frame_support::debug::trace!("Message being dispatched is: {:?}", &message.call);
		let dispatch_result = message.call.dispatch(origin);
		let actual_call_weight = extract_actual_weight(&dispatch_result, &dispatch_info);

		frame_support::debug::trace!(
			"Message {:?}/{:?} has been dispatched. Weight: {} of {}. Result: {:?}",
			bridge,
			id,
			actual_call_weight,
			message.weight,
			dispatch_result,
		);

		Self::deposit_event(RawEvent::MessageDispatched(
			bridge,
			id,
			dispatch_result.map(drop).map_err(|e| e.error),
		));
	}
}

/// Check if the message is allowed to be dispatched on the target chain given the sender's origin
/// on the source chain.
///
/// For example, if a message is sent from a "regular" account on the source chain it will not be
/// allowed to be dispatched as Root on the target chain. This is a useful check to do on the source
/// chain _before_ sending a message whose dispatch will be rejected on the target chain.
pub fn verify_message_origin<SourceChainAccountId, TargetChainAccountPublic, TargetChainSignature, Call>(
	sender_origin: &RawOrigin<SourceChainAccountId>,
	message: &MessagePayload<SourceChainAccountId, TargetChainAccountPublic, TargetChainSignature, Call>,
) -> Result<Option<SourceChainAccountId>, BadOrigin>
where
	SourceChainAccountId: PartialEq + Clone,
{
	match message.origin {
		CallOrigin::SourceRoot => {
			ensure!(sender_origin == &RawOrigin::Root, BadOrigin);
			Ok(None)
		}
		CallOrigin::TargetAccount(ref source_account_id, _, _) => {
			ensure!(
				sender_origin == &RawOrigin::Signed(source_account_id.clone()),
				BadOrigin
			);
			Ok(Some(source_account_id.clone()))
		}
		CallOrigin::SourceAccount(ref source_account_id) => {
			ensure!(
				sender_origin == &RawOrigin::Signed(source_account_id.clone()),
				BadOrigin
			);
			Ok(Some(source_account_id.clone()))
		}
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
		Perbill,
	};

	type AccountId = u64;
	type CallDispatch = Module<TestRuntime>;
	type System = frame_system::Module<TestRuntime>;

	type MessageId = [u8; 4];

	#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq)]
	pub struct TestAccountPublic(AccountId);

	impl IdentifyAccount for TestAccountPublic {
		type AccountId = AccountId;

		fn into_account(self) -> AccountId {
			self.0
		}
	}

	#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq)]
	pub struct TestSignature(AccountId);

	impl Verify for TestSignature {
		type Signer = TestAccountPublic;

		fn verify<L: sp_runtime::traits::Lazy<[u8]>>(&self, _msg: L, signer: &AccountId) -> bool {
			self.0 == *signer
		}
	}

	pub struct AccountIdConverter;

	impl sp_runtime::traits::Convert<H256, AccountId> for AccountIdConverter {
		fn convert(hash: H256) -> AccountId {
			hash.to_low_u64_ne()
		}
	}

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

	impl frame_system::Config for TestRuntime {
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
		type Version = ();
		type PalletInfo = ();
		type AccountData = ();
		type OnNewAccount = ();
		type OnKilledAccount = ();
		type BaseCallFilter = ();
		type SystemWeightInfo = ();
		type BlockWeights = ();
		type BlockLength = ();
		type DbWeight = ();
	}

	impl Config for TestRuntime {
		type Event = TestEvent;
		type MessageId = MessageId;
		type SourceChainAccountId = AccountId;
		type TargetChainAccountPublic = TestAccountPublic;
		type TargetChainSignature = TestSignature;
		type Call = Call;
		type AccountIdConverter = AccountIdConverter;
	}

	const TEST_SPEC_VERSION: SpecVersion = 0;
	const TEST_WEIGHT: Weight = 1_000_000_000;

	fn new_test_ext() -> sp_io::TestExternalities {
		let t = frame_system::GenesisConfig::default()
			.build_storage::<TestRuntime>()
			.unwrap();
		sp_io::TestExternalities::new(t)
	}

	fn prepare_message(
		origin: CallOrigin<AccountId, TestAccountPublic, TestSignature>,
		call: Call,
	) -> <Module<TestRuntime> as MessageDispatch<<TestRuntime as Config>::MessageId>>::Message {
		MessagePayload {
			spec_version: TEST_SPEC_VERSION,
			weight: TEST_WEIGHT,
			origin,
			call,
		}
	}

	fn prepare_root_message(
		call: Call,
	) -> <Module<TestRuntime> as MessageDispatch<<TestRuntime as Config>::MessageId>>::Message {
		prepare_message(CallOrigin::SourceRoot, call)
	}

	fn prepare_target_message(
		call: Call,
	) -> <Module<TestRuntime> as MessageDispatch<<TestRuntime as Config>::MessageId>>::Message {
		let origin = CallOrigin::TargetAccount(1, TestAccountPublic(1), TestSignature(1));
		prepare_message(origin, call)
	}

	fn prepare_source_message(
		call: Call,
	) -> <Module<TestRuntime> as MessageDispatch<<TestRuntime as Config>::MessageId>>::Message {
		let origin = CallOrigin::SourceAccount(1);
		prepare_message(origin, call)
	}

	#[test]
	fn should_fail_on_spec_version_mismatch() {
		new_test_ext().execute_with(|| {
			let bridge = b"ethb".to_owned();
			let id = [0; 4];

			const BAD_SPEC_VERSION: SpecVersion = 99;
			let mut message =
				prepare_root_message(Call::System(<frame_system::Call<TestRuntime>>::remark(vec![1, 2, 3])));
			message.spec_version = BAD_SPEC_VERSION;

			System::set_block_number(1);
			CallDispatch::dispatch(bridge, id, message);

			assert_eq!(
				System::events(),
				vec![EventRecord {
					phase: Phase::Initialization,
					event: TestEvent::call_dispatch(Event::<TestRuntime>::MessageVersionSpecMismatch(
						bridge,
						id,
						TEST_SPEC_VERSION,
						BAD_SPEC_VERSION
					)),
					topics: vec![],
				}],
			);
		});
	}

	#[test]
	fn should_fail_on_weight_mismatch() {
		new_test_ext().execute_with(|| {
			let bridge = b"ethb".to_owned();
			let id = [0; 4];
			let mut message =
				prepare_root_message(Call::System(<frame_system::Call<TestRuntime>>::remark(vec![1, 2, 3])));
			message.weight = 0;

			System::set_block_number(1);
			CallDispatch::dispatch(bridge, id, message);

			assert_eq!(
				System::events(),
				vec![EventRecord {
					phase: Phase::Initialization,
					event: TestEvent::call_dispatch(Event::<TestRuntime>::MessageWeightMismatch(
						bridge, id, 1973000, 0,
					)),
					topics: vec![],
				}],
			);
		});
	}

	#[test]
	fn should_fail_on_signature_mismatch() {
		new_test_ext().execute_with(|| {
			let bridge = b"ethb".to_owned();
			let id = [0; 4];

			let call_origin = CallOrigin::TargetAccount(1, TestAccountPublic(1), TestSignature(99));
			let message = prepare_message(
				call_origin,
				Call::System(<frame_system::Call<TestRuntime>>::remark(vec![1, 2, 3])),
			);

			System::set_block_number(1);
			CallDispatch::dispatch(bridge, id, message);

			assert_eq!(
				System::events(),
				vec![EventRecord {
					phase: Phase::Initialization,
					event: TestEvent::call_dispatch(Event::<TestRuntime>::MessageSignatureMismatch(bridge, id)),
					topics: vec![],
				}],
			);
		});
	}

	#[test]
	fn should_dispatch_bridge_message_from_root_origin() {
		new_test_ext().execute_with(|| {
			let bridge = b"ethb".to_owned();
			let id = [0; 4];
			let message = prepare_root_message(Call::System(<frame_system::Call<TestRuntime>>::remark(vec![1, 2, 3])));

			System::set_block_number(1);
			CallDispatch::dispatch(bridge, id, message);

			assert_eq!(
				System::events(),
				vec![EventRecord {
					phase: Phase::Initialization,
					event: TestEvent::call_dispatch(Event::<TestRuntime>::MessageDispatched(bridge, id, Ok(()))),
					topics: vec![],
				}],
			);
		});
	}

	#[test]
	fn should_dispatch_bridge_message_from_target_origin() {
		new_test_ext().execute_with(|| {
			let id = [0; 4];
			let bridge = b"ethb".to_owned();

			let call = Call::System(<frame_system::Call<TestRuntime>>::remark(vec![]));
			let message = prepare_target_message(call);

			System::set_block_number(1);
			CallDispatch::dispatch(bridge, id, message);

			assert_eq!(
				System::events(),
				vec![EventRecord {
					phase: Phase::Initialization,
					event: TestEvent::call_dispatch(Event::<TestRuntime>::MessageDispatched(bridge, id, Ok(()))),
					topics: vec![],
				}],
			);
		})
	}

	#[test]
	fn should_dispatch_bridge_message_from_source_origin() {
		new_test_ext().execute_with(|| {
			let id = [0; 4];
			let bridge = b"ethb".to_owned();

			let call = Call::System(<frame_system::Call<TestRuntime>>::remark(vec![]));
			let message = prepare_source_message(call);

			System::set_block_number(1);
			CallDispatch::dispatch(bridge, id, message);

			assert_eq!(
				System::events(),
				vec![EventRecord {
					phase: Phase::Initialization,
					event: TestEvent::call_dispatch(Event::<TestRuntime>::MessageDispatched(bridge, id, Ok(()))),
					topics: vec![],
				}],
			);
		})
	}

	#[test]
	fn origin_is_checked_when_verifying_sending_message_using_source_root_account() {
		let call = Call::System(<frame_system::Call<TestRuntime>>::remark(vec![]));
		let message = prepare_root_message(call);

		// When message is sent by Root, CallOrigin::SourceRoot is allowed
		assert!(matches!(verify_message_origin(&RawOrigin::Root, &message), Ok(None)));

		// when message is sent by some real account, CallOrigin::SourceRoot is not allowed
		assert!(matches!(
			verify_message_origin(&RawOrigin::Signed(1), &message),
			Err(BadOrigin)
		));
	}

	#[test]
	fn origin_is_checked_when_verifying_sending_message_using_target_account() {
		let call = Call::System(<frame_system::Call<TestRuntime>>::remark(vec![]));
		let message = prepare_target_message(call);

		// When message is sent by Root, CallOrigin::TargetAccount is not allowed
		assert!(matches!(
			verify_message_origin(&RawOrigin::Root, &message),
			Err(BadOrigin)
		));

		// When message is sent by some other account, it is rejected
		assert!(matches!(
			verify_message_origin(&RawOrigin::Signed(2), &message),
			Err(BadOrigin)
		));

		// When message is sent by a real account, it is allowed to have origin
		// CallOrigin::TargetAccount
		assert!(matches!(
			verify_message_origin(&RawOrigin::Signed(1), &message),
			Ok(Some(1))
		));
	}

	#[test]
	fn origin_is_checked_when_verifying_sending_message_using_source_account() {
		let call = Call::System(<frame_system::Call<TestRuntime>>::remark(vec![]));
		let message = prepare_source_message(call);

		// Sending a message from the expected origin account works
		assert!(matches!(
			verify_message_origin(&RawOrigin::Signed(1), &message),
			Ok(Some(1))
		));

		// If we send a message from a different account, it is rejected
		assert!(matches!(
			verify_message_origin(&RawOrigin::Signed(2), &message),
			Err(BadOrigin)
		));

		// If we try and send the message from Root, it is also rejected
		assert!(matches!(
			verify_message_origin(&RawOrigin::Root, &message),
			Err(BadOrigin)
		));
	}
}
