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
use codec::{Decode, Encode};
use frame_support::{
	decl_event, decl_module, decl_storage,
	dispatch::{Dispatchable, Parameter},
	traits::Get,
	weights::{extract_actual_weight, GetDispatchInfo},
	RuntimeDebug,
};
use frame_system::{ensure_root, ensure_signed, RawOrigin};
use sp_runtime::{
	traits::{BadOrigin, IdentifyAccount, Verify},
	DispatchResult,
};
use sp_std::{marker::PhantomData, prelude::*};

/// Spec version type.
pub type SpecVersion = u32;

/// Origin of the call on the target chain.
#[derive(RuntimeDebug, Encode, Decode, Clone, PartialEq, Eq)]
pub enum CallOrigin<SourceChainAccountPublic, TargetChainAccountPublic, TargetChainSignature> {
	/// Call is originated from bridge account, which is (designed to be) specific to
	/// the single deployed instance of the messages bridge (message-lane, ...) module.
	/// It is assumed that this account is not controlled by anyone and has zero balance
	/// (unless someone would make transfer by mistake?).
	/// If we trust the source chain to allow sending calls with that origin in case they originate
	/// from source chain `root` account (default implementation), `BridgeAccount` represents the
	/// source-chain-root origin on the target chain and can be used to send and authorize
	/// "control plane" messages between the two runtimes.
	BridgeAccount,
	/// Call is originated from account, identified by `TargetChainAccountPublic`. The proof
	/// that the `SourceChainAccountPublic` controls `TargetChainAccountPublic` is the
	/// `TargetChainSignature` over `(Call, SourceChainAccountPublic).encode()`.
	/// The source chain must ensure that the message is sent by the owner of
	/// `SourceChainAccountPublic` account (use the `fn verify_sending_message()`).
	RealAccount(SourceChainAccountPublic, TargetChainAccountPublic, TargetChainSignature),
}

/// Message payload type used by call-dispatch module.
#[derive(RuntimeDebug, Encode, Decode, Clone, PartialEq, Eq)]
pub struct MessagePayload<SourceChainAccountPublic, TargetChainAccountPublic, TargetChainSignature, Call> {
	/// Runtime specification version. We only dispatch messages that have the same
	/// runtime version. Otherwise we risk to misinterpret encoded calls.
	pub spec_version: SpecVersion,
	/// Weight of the call, declared by the message sender. If it is less than actual
	/// static weight, the call is not dispatched.
	pub weight: Weight,
	/// Call origin to be used during dispatch.
	pub origin: CallOrigin<SourceChainAccountPublic, TargetChainAccountPublic, TargetChainSignature>,
	/// The call itself.
	pub call: Call,
}

/// The module configuration trait.
pub trait Trait<I = DefaultInstance>: frame_system::Trait {
	/// The overarching event type.
	type Event: From<Event<Self, I>> + Into<<Self as frame_system::Trait>::Event>;
	/// Id of the message. Whenever message is passed to the dispatch module, it emits
	/// event with this id + dispatch result. Could be e.g. (LaneId, MessageNonce) if
	/// it comes from message-lane module.
	type MessageId: Parameter;
	/// Type of account public key on source chain.
	type SourceChainAccountPublic: Parameter;
	/// Type of account public key on target chain.
	type TargetChainAccountPublic: Parameter + IdentifyAccount<AccountId = Self::AccountId>;
	/// Type of signature that may prove that the message has been signed by
	/// owner of `TargetChainAccountPublic`.
	type TargetChainSignature: Parameter + Verify<Signer = Self::TargetChainAccountPublic>;
	/// The overarching dispatch call type.
	type Call: Parameter
		+ GetDispatchInfo
		+ Dispatchable<
			Origin = <Self as frame_system::Trait>::Origin,
			PostInfo = frame_support::dispatch::PostDispatchInfo,
		>;
}

decl_storage! {
	trait Store for Module<T: Trait<I>, I: Instance = DefaultInstance> as CallDispatch {
	}
}

decl_event!(
	pub enum Event<T, I = DefaultInstance> where
		<T as Trait<I>>::MessageId
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
		/// Phantom member, never used.
		Dummy(PhantomData<I>),
	}
);

decl_module! {
	/// Call Dispatch FRAME Pallet.
	pub struct Module<T: Trait<I>, I: Instance = DefaultInstance> for enum Call where origin: T::Origin {
		/// Deposit one of this module's events by using the default implementation.
		fn deposit_event() = default;
	}
}

impl<T: Trait<I>, I: Instance> MessageDispatch<T::MessageId> for Module<T, I> {
	type Message = MessagePayload<
		T::SourceChainAccountPublic,
		T::TargetChainAccountPublic,
		T::TargetChainSignature,
		<T as Trait<I>>::Call,
	>;

	fn dispatch_weight(message: &Self::Message) -> Weight {
		message.weight
	}

	fn dispatch(bridge: InstanceId, id: T::MessageId, message: Self::Message) {
		// verify spec version
		// (we want it to be the same, because otherwise we may decode Call improperly)
		let expected_version = <T as frame_system::Trait>::Version::get().spec_version;
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
			CallOrigin::BridgeAccount => bridge_account_id(bridge, CALL_DISPATCH_MODULE_PREFIX),
			CallOrigin::RealAccount(source_public, target_public, target_signature) => {
				let mut signed_message = Vec::new();
				message.call.encode_to(&mut signed_message);
				source_public.encode_to(&mut signed_message);

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

				target_account
			}
		};

		// finally dispatch message
		let origin = RawOrigin::Signed(origin_account).into();
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

/// Verify payload of the message at the sending side.
pub fn verify_sending_message<
	ThisChainOuterOrigin,
	ThisChainAccountId,
	SourceChainAccountPublic,
	TargetChainAccountPublic,
	TargetChainSignature,
	Call,
>(
	sender_origin: ThisChainOuterOrigin,
	message: &MessagePayload<TargetChainAccountPublic, SourceChainAccountPublic, TargetChainSignature, Call>,
) -> Result<Option<ThisChainAccountId>, BadOrigin>
where
	ThisChainOuterOrigin: Into<Result<RawOrigin<ThisChainAccountId>, ThisChainOuterOrigin>>,
	TargetChainAccountPublic: Clone + IdentifyAccount<AccountId = ThisChainAccountId>,
	ThisChainAccountId: PartialEq,
{
	match message.origin {
		CallOrigin::BridgeAccount => {
			ensure_root(sender_origin)?;
			Ok(None)
		}
		CallOrigin::RealAccount(ref this_account_public, _, _) => {
			let this_chain_account_id = ensure_signed(sender_origin)?;
			if this_chain_account_id != this_account_public.clone().into_account() {
				return Err(BadOrigin);
			}

			Ok(Some(this_chain_account_id))
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
		DispatchError, Perbill,
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
		type PalletInfo = ();
		type AccountData = ();
		type OnNewAccount = ();
		type OnKilledAccount = ();
		type BaseCallFilter = ();
		type SystemWeightInfo = ();
	}

	impl Trait for TestRuntime {
		type Event = TestEvent;
		type MessageId = MessageId;
		type SourceChainAccountPublic = TestAccountPublic;
		type TargetChainAccountPublic = TestAccountPublic;
		type TargetChainSignature = TestSignature;
		type Call = Call;
	}

	const TEST_SPEC_VERSION: SpecVersion = 0;
	const TEST_WEIGHT: Weight = 1_000_000_000;

	fn new_test_ext() -> sp_io::TestExternalities {
		let t = frame_system::GenesisConfig::default()
			.build_storage::<TestRuntime>()
			.unwrap();
		sp_io::TestExternalities::new(t)
	}

	fn prepare_bridge_message(
		call: Call,
	) -> <Module<TestRuntime> as MessageDispatch<<TestRuntime as Trait>::MessageId>>::Message {
		MessagePayload {
			spec_version: TEST_SPEC_VERSION,
			weight: TEST_WEIGHT,
			origin: CallOrigin::BridgeAccount,
			call,
		}
	}

	#[test]
	fn should_succesfuly_dispatch_remark() {
		new_test_ext().execute_with(|| {
			let origin = b"ethb".to_owned();
			let id = [0; 4];
			let message =
				prepare_bridge_message(Call::System(<frame_system::Call<TestRuntime>>::remark(vec![1, 2, 3])));

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
			let mut message =
				prepare_bridge_message(Call::System(<frame_system::Call<TestRuntime>>::remark(vec![1, 2, 3])));
			message.origin = CallOrigin::RealAccount(TestAccountPublic(2), TestAccountPublic(2), TestSignature(1));

			System::set_block_number(1);
			CallDispatch::dispatch(origin, id, message);

			assert_eq!(
				System::events(),
				vec![EventRecord {
					phase: Phase::Initialization,
					event: TestEvent::call_dispatch(Event::<TestRuntime>::MessageSignatureMismatch(origin, id,)),
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
			let mut message =
				prepare_bridge_message(Call::System(<frame_system::Call<TestRuntime>>::remark(vec![1, 2, 3])));
			message.weight = 0;

			System::set_block_number(1);
			CallDispatch::dispatch(origin, id, message);

			assert_eq!(
				System::events(),
				vec![EventRecord {
					phase: Phase::Initialization,
					event: TestEvent::call_dispatch(Event::<TestRuntime>::MessageWeightMismatch(
						origin, id, 1973000, 0,
					)),
					topics: vec![],
				}],
			);
		});
	}

	#[test]
	fn should_fail_on_signature_mismatch() {
		new_test_ext().execute_with(|| {
			let origin = b"ethb".to_owned();
			let id = [0; 4];
			let mut message =
				prepare_bridge_message(Call::System(<frame_system::Call<TestRuntime>>::remark(vec![1, 2, 3])));
			message.weight = 0;

			System::set_block_number(1);
			CallDispatch::dispatch(origin, id, message);

			assert_eq!(
				System::events(),
				vec![EventRecord {
					phase: Phase::Initialization,
					event: TestEvent::call_dispatch(Event::<TestRuntime>::MessageWeightMismatch(
						origin, id, 1973000, 0,
					)),
					topics: vec![],
				}],
			);
		});
	}

	#[test]
	fn should_dispatch_bridge_message_from_non_root_origin() {
		new_test_ext().execute_with(|| {
			let origin = b"ethb".to_owned();
			let id = [0; 4];
			let message = prepare_bridge_message(Call::System(<frame_system::Call<TestRuntime>>::fill_block(
				Perbill::from_percent(10),
			)));

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

	#[test]
	fn dispatch_supports_different_accounts() {
		fn dispatch_suicide(call_origin: CallOrigin<TestAccountPublic, TestAccountPublic, TestSignature>) {
			let origin = b"ethb".to_owned();
			let id = [0; 4];
			let mut message = prepare_bridge_message(Call::System(<frame_system::Call<TestRuntime>>::suicide()));
			message.origin = call_origin;

			System::set_block_number(1);
			CallDispatch::dispatch(origin, id, message);
		}

		new_test_ext().execute_with(|| {
			// 'create' real account
			let real_account_id = 1;
			System::inc_account_nonce(real_account_id);
			// 'create' bridge account
			let bridge_account_id: AccountId = bridge_account_id(*b"ethb", CALL_DISPATCH_MODULE_PREFIX);
			System::inc_account_nonce(bridge_account_id);

			assert_eq!(System::account_nonce(real_account_id), 1);
			assert_eq!(System::account_nonce(bridge_account_id), 1);

			// kill real account
			dispatch_suicide(CallOrigin::RealAccount(
				TestAccountPublic(real_account_id),
				TestAccountPublic(real_account_id),
				TestSignature(real_account_id),
			));
			assert_eq!(System::account_nonce(real_account_id), 0);
			assert_eq!(System::account_nonce(bridge_account_id), 1);

			// kill bridge account
			dispatch_suicide(CallOrigin::BridgeAccount);
			assert_eq!(System::account_nonce(real_account_id), 0);
			assert_eq!(System::account_nonce(bridge_account_id), 0);
		});
	}

	#[test]
	fn origin_is_checked_when_verify_sending_message() {
		let mut message = prepare_bridge_message(Call::System(<frame_system::Call<TestRuntime>>::suicide()));

		// when message is sent by root, CallOrigin::BridgeAccount is allowed
		assert!(matches!(
			verify_sending_message(Origin::from(RawOrigin::Root), &message),
			Ok(None)
		));

		// when message is sent by some real account, CallOrigin::BridgeAccount is not allowed
		assert!(matches!(
			verify_sending_message(Origin::from(RawOrigin::Signed(1)), &message),
			Err(BadOrigin)
		));

		// when message is sent by root, CallOrigin::RealAccount is not allowed
		message.origin = CallOrigin::RealAccount(TestAccountPublic(2), TestAccountPublic(2), TestSignature(2));
		assert!(matches!(
			verify_sending_message(Origin::from(RawOrigin::Root), &message),
			Err(BadOrigin)
		));

		// when message is sent by some other account, it is rejected
		assert!(matches!(
			verify_sending_message(Origin::from(RawOrigin::Signed(1)), &message),
			Err(BadOrigin)
		));

		// when message is sent real account, it is allowed to have origin CallOrigin::RealAccount
		assert!(matches!(
			verify_sending_message(Origin::from(RawOrigin::Signed(2)), &message),
			Ok(Some(2))
		));
	}
}
