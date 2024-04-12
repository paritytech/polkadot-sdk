// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//  http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Transaction Extensions and types to support meta transactions.

use crate::*;
use codec::Codec;
use core::fmt::Debug;
use frame_support::{CloneNoBound, EqNoBound, PartialEqNoBound, RuntimeDebugNoBound};
use scale_info::StaticTypeInfo;
use sp_io::hashing::blake2_256;
use sp_runtime::{
	impl_tx_ext_default,
	traits::{IdentifyAccount, Verify},
};

/// Describes the relayer constraints of a transaction, which can be any account or a specific
/// account.
#[derive(Clone, Default, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo)]
pub enum TxRelayer<AccountId> {
	/// Transaction fee must be paid by relayer.
	#[default]
	AnyRelayer,
	/// Transaction fee must be paid by a given account.
	Relayer(AccountId),
}

/// This requires the transactor to pay for themselves or for the signer in the case of a meta
/// transaction. It may also include a tip to gain additional priority in the queue.
#[derive(Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct RelayTransactionPayment<T: Config>(
	#[codec(compact)] BalanceOf<T>,
	Option<TxRelayer<T::AccountId>>,
);

impl<T: Config> RelayTransactionPayment<T>
where
	BalanceOf<T>: Send + Sync,
{
	/// utility constructor. Used only in client/factory code.
	pub fn from(fee: BalanceOf<T>) -> Self {
		Self(fee, None)
	}
	/// Create with a relayer constraint.
	pub fn with_any_relayer() -> Self {
		Self(BalanceOf::<T>::zero(), Some(TxRelayer::AnyRelayer))
	}
	/// Create with a specific authorized relayer.
	pub fn with_relayer(account: T::AccountId) -> Self {
		Self(BalanceOf::<T>::zero(), Some(TxRelayer::Relayer(account)))
	}
	/// Returns the tip as being chosen by the transaction sender.
	pub fn tip(&self) -> BalanceOf<T> {
		self.0
	}
	/// Returns the relayer constraints.
	pub fn relayer(&self) -> Option<&TxRelayer<T::AccountId>> {
		self.1.as_ref()
	}
}

impl<T: Config> sp_std::fmt::Debug for RelayTransactionPayment<T> {
	#[cfg(feature = "std")]
	fn fmt(&self, f: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
		write!(f, "RelayTransactionPayment<{:?}, {:?}>", self.0, self.1)
	}
	#[cfg(not(feature = "std"))]
	fn fmt(&self, _: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
		Ok(())
	}
}

impl<T: Config> TransactionExtensionBase for RelayTransactionPayment<T> {
	const IDENTIFIER: &'static str = "RelayTransactionPayment";
	type Implicit = ();

	fn weight(&self) -> Weight {
		T::WeightInfo::charge_transaction_payment()
	}
}

impl<T: Config, Context> TransactionExtension<T::RuntimeCall, Context> for RelayTransactionPayment<T>
where
	BalanceOf<T>: Send + Sync + From<u64>,
	T::RuntimeCall: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>,
	Context: meta_tx::GetTxRelayer<T::AccountId>,
{
	type Val = (
		// tip
		BalanceOf<T>,
		// who paid the fee
		T::AccountId,
		// computed fee
		BalanceOf<T>,
	);
	type Pre = (
		// tip
		BalanceOf<T>,
		// who paid the fee
		T::AccountId,
		// imbalance resulting from withdrawing the fee
		<<T as Config>::OnChargeTransaction as OnChargeTransaction<T>>::LiquidityInfo,
	);

	fn validate(
		&self,
		origin: <T::RuntimeCall as Dispatchable>::RuntimeOrigin,
		call: &T::RuntimeCall,
		info: &DispatchInfoOf<T::RuntimeCall>,
		len: usize,
		context: &mut Context,
		_: (),
		_implication: &impl Encode,
	) -> Result<
		(ValidTransaction, Self::Val, <T::RuntimeCall as Dispatchable>::RuntimeOrigin),
		TransactionValidityError,
	> {
		use TxRelayer::*;
		let who = if let Some(authorized) = &self.relayer() {
			let relayer = context.get_relayer().ok_or(InvalidTransaction::BadSigner)?;
			match authorized {
				AnyRelayer => relayer,
				Relayer(authorized) if authorized == &relayer => relayer,
				_ => return Err(InvalidTransaction::BadSigner.into()),
			}
		} else {
			frame_system::ensure_signed(origin.clone())
				.map_err(|_| InvalidTransaction::BadSigner)?
		};

		let tip = self.0;

		let final_fee = {
			let fee = Pallet::<T>::compute_fee(len as u32, info, tip);
			<<T as Config>::OnChargeTransaction as OnChargeTransaction<T>>::can_withdraw_fee(
				&who, call, info, fee, tip,
			)?;
			fee
		};

		Ok((
			ValidTransaction {
				priority: Priority::<T>::get_priority(info, len, tip, final_fee),
				..Default::default()
			},
			(self.0, who, final_fee),
			origin,
		))
	}

	fn prepare(
		self,
		val: Self::Val,
		_origin: &<T::RuntimeCall as Dispatchable>::RuntimeOrigin,
		call: &T::RuntimeCall,
		info: &DispatchInfoOf<T::RuntimeCall>,
		_len: usize,
		_context: &Context,
	) -> Result<Self::Pre, TransactionValidityError> {
		let (tip, who, fee) = val;
		// Mutating call to `withdraw_fee` to actually charge for the transaction.
		let imbalance =
			<<T as Config>::OnChargeTransaction as OnChargeTransaction<T>>::withdraw_fee(
				&who, call, info, fee, tip,
			)?;

		Ok((tip, who, imbalance))
	}

	fn post_dispatch(
		(tip, who, imbalance): Self::Pre,
		info: &DispatchInfoOf<T::RuntimeCall>,
		post_info: &PostDispatchInfoOf<T::RuntimeCall>,
		len: usize,
		_result: &DispatchResult,
		_context: &Context,
	) -> Result<(), TransactionValidityError> {
		let actual_fee = Pallet::<T>::compute_actual_fee(len as u32, info, post_info, tip);
		T::OnChargeTransaction::correct_and_deposit_fee(
			&who, info, post_info, actual_fee, tip, imbalance,
		)?;
		Pallet::<T>::deposit_event(Event::<T>::TransactionFeePaid { who, actual_fee, tip });
		Ok(())
	}
}

/// A trait for getting the relayer of a transaction from a context.
pub trait GetTxRelayer<AccountId> {
	/// Returns the relayer of the transaction, if any.
	fn get_relayer(&self) -> Option<AccountId>;
}

/// Implementation of the `GetTxRelayer` trait for the unit type `()`.
impl<AccountId> GetTxRelayer<AccountId> for () {
	/// Returns `None` as there is no relayer for the unit type `()`.
	fn get_relayer(&self) -> Option<AccountId> {
		None
	}
}

/// A trait for setting the relayer of a transaction to a context.
pub trait SetTxRelayer<AccountId> {
	/// Sets the relayer of the transaction.
	fn set_relayer(&mut self, relayer: AccountId);
}

/// Represents the context of a transaction, including the relayer account ID if available.
#[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo)]
pub struct Context<AccountId: Clone> {
	relayer: Option<AccountId>,
}
impl<AccountId: Clone> Default for Context<AccountId> {
	fn default() -> Self {
		Self { relayer: None }
	}
}
impl<AccountId: Clone> GetTxRelayer<AccountId> for Context<AccountId> {
	fn get_relayer(&self) -> Option<AccountId> {
		self.relayer.clone()
	}
}
impl<AccountId: Clone> SetTxRelayer<AccountId> for Context<AccountId> {
	fn set_relayer(&mut self, relayer: AccountId) {
		self.relayer = Some(relayer);
	}
}

/// Transaction extension that sets the relayer of the transaction, the account sponsoring the
/// transaction fee, if that signature was derived from the `inherited_implication`, which contains
/// the call and all subsequent extensions. If signature is not provided, this extension is no-op.
#[derive(
	CloneNoBound, EqNoBound, PartialEqNoBound, Encode, Decode, RuntimeDebugNoBound, TypeInfo,
)]
#[codec(encode_bound())]
#[codec(decode_bound())]
pub struct VerifyRelayerSignature<V: Verify>
where
	V: Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
	<V::Signer as IdentifyAccount>::AccountId:
		Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
{
	pub relayer: Option<(V, <V::Signer as IdentifyAccount>::AccountId)>,
}

impl<V: Verify> Default for VerifyRelayerSignature<V>
where
	V: Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
	<V::Signer as IdentifyAccount>::AccountId:
		Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
{
	fn default() -> Self {
		Self { relayer: None }
	}
}

impl<V: Verify> VerifyRelayerSignature<V>
where
	V: Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
	<V::Signer as IdentifyAccount>::AccountId:
		Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
{
	pub fn new_with_relayer(
		signature: V,
		relayer: <V::Signer as IdentifyAccount>::AccountId,
	) -> Self {
		Self { relayer: Some((signature, relayer)) }
	}
}

impl<V: Verify> TransactionExtensionBase for VerifyRelayerSignature<V>
where
	V: Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
	<V::Signer as IdentifyAccount>::AccountId:
		Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
{
	const IDENTIFIER: &'static str = "VerifyRelayerSignature";
	type Implicit = ();
}

impl<Call: Dispatchable, V: Verify, Context> TransactionExtension<Call, Context>
	for VerifyRelayerSignature<V>
where
	V: Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
	<V::Signer as IdentifyAccount>::AccountId:
		Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
	Context: SetTxRelayer<<V::Signer as IdentifyAccount>::AccountId>,
{
	type Val = ();
	type Pre = ();
	impl_tx_ext_default!(Call; Context; prepare);

	fn validate(
		&self,
		origin: <Call as Dispatchable>::RuntimeOrigin,
		_call: &Call,
		_info: &DispatchInfoOf<Call>,
		_len: usize,
		context: &mut Context,
		_: (),
		inherited_implication: &impl Encode,
	) -> Result<
		(ValidTransaction, Self::Val, <Call as Dispatchable>::RuntimeOrigin),
		TransactionValidityError,
	> {
		let (signature, relayer) = match &self.relayer {
			None => return Ok((ValidTransaction::default(), (), origin)),
			Some((s, a)) => (s, a.clone()), // TODO check if origin None
		};

		let msg = inherited_implication.using_encoded(blake2_256);

		if !signature.verify(&msg[..], &relayer) {
			Err(InvalidTransaction::BadProof)?
		}
		context.set_relayer(relayer);
		Ok((ValidTransaction::default(), (), origin))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use frame_support::{
		construct_runtime, derive_impl,
		traits::{fungible::Inspect, BuildGenesisConfig},
		weights::{FixedFee, NoFee},
	};
	use keyring::AccountKeyring;
	use sp_core::ConstU8;
	use sp_runtime::{
		traits::{Applyable, Checkable, Hash, IdentityLookup},
		MultiSignature,
	};

	pub type Balance = u64;

	pub type Signature = MultiSignature;
	pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

	pub type UncheckedExtrinsic = sp_runtime::generic::UncheckedExtrinsic<
		AccountId,
		RuntimeCall,
		Signature,
		TxExtension,
		Context<AccountId>,
	>;

	pub type TxExtension = (VerifyRelayerSignature<sp_runtime::MultiSignature>, SignedTxExtension);

	// The part of `TxExtension` that has to be provided and signed by the transaction relayer,
	// the user who sponsors the transaction fee.
	type SignedTxExtension = (
		frame_support::transaction_extensions::VerifyAccountSignature<MultiSignature>,
		MetaTxExtension,
	);

	// The part of `TxExtension` that has to be provided and signed by user who wants
	// the transaction fee to be sponsored by someone else.
	type MetaTxExtension = (
		frame_system::CheckNonZeroSender<Runtime>,
		frame_system::CheckSpecVersion<Runtime>,
		frame_system::CheckTxVersion<Runtime>,
		frame_system::CheckGenesis<Runtime>,
		frame_system::CheckMortality<Runtime>,
		frame_system::CheckNonce<Runtime>,
		frame_system::CheckWeight<Runtime>,
		RelayTransactionPayment<Runtime>,
	);

	#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
	impl frame_system::Config for Runtime {
		type AccountId = AccountId;
		type Lookup = IdentityLookup<Self::AccountId>;
		type Block = frame_system::mocking::MockBlock<Runtime>;
		type AccountData = pallet_balances::AccountData<<Self as pallet_balances::Config>::Balance>;
	}

	#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
	impl pallet_balances::Config for Runtime {
		type ReserveIdentifier = [u8; 8];
		type AccountStore = System;
	}

	pub const TX_FEE: u32 = 10;

	impl Config for Runtime {
		type WeightInfo = ();
		type RuntimeEvent = RuntimeEvent;
		type OnChargeTransaction = CurrencyAdapter<Balances, ()>;
		type OperationalFeeMultiplier = ConstU8<1>;
		type WeightToFee = FixedFee<TX_FEE, Balance>;
		type LengthToFee = NoFee<Balance>;
		type FeeMultiplierUpdate = ();
	}

	construct_runtime!(
		pub enum Runtime {
			System: frame_system,
			Balances: pallet_balances,
			TxPayment: crate,
		}
	);

	pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
		let mut ext = sp_io::TestExternalities::new(Default::default());
		ext.execute_with(|| {
			frame_system::GenesisConfig::<Runtime>::default().build();
			System::set_block_number(1);
		});
		ext
	}

	#[test]
	fn meta_tx_works() {
		new_test_ext().execute_with(|| {
			// meta tx signer
			let alice_keyring = AccountKeyring::Alice;
			// meta tx relayer
			let bob_keyring = AccountKeyring::Bob;

			let alice_account = AccountId::from(alice_keyring.public());
			let bob_account = AccountId::from(bob_keyring.public());

			let ed = Balances::minimum_balance();
			let tx_fee: Balance = (2 * TX_FEE).into(); // base tx fee + weight fee
			let alice_balance = ed * 100;
			let bob_balance = ed * 100;

			{
				// setup initial balances for alice and bob
				Balances::force_set_balance(
					RuntimeOrigin::root(),
					alice_account.clone().into(),
					alice_balance,
				)
				.unwrap();
				Balances::force_set_balance(
					RuntimeOrigin::root(),
					bob_account.clone().into(),
					bob_balance,
				)
				.unwrap();
			}

			// Alice builds a meta transaction.

			let remark_call =
				RuntimeCall::System(frame_system::Call::remark_with_event { remark: vec![1] });
			let meta_tx_ext: MetaTxExtension = (
				frame_system::CheckNonZeroSender::<Runtime>::new(),
				frame_system::CheckSpecVersion::<Runtime>::new(),
				frame_system::CheckTxVersion::<Runtime>::new(),
				frame_system::CheckGenesis::<Runtime>::new(),
				frame_system::CheckMortality::<Runtime>::from(sp_runtime::generic::Era::immortal()),
				frame_system::CheckNonce::<Runtime>::from(
					frame_system::Pallet::<Runtime>::account(&alice_account).nonce,
				),
				frame_system::CheckWeight::<Runtime>::new(),
				RelayTransactionPayment::with_relayer(bob_account.clone()),
				// or can be RelayTransactionPayment::with_any_relayer(),
			);

			let meta_tx_sig = MultiSignature::Sr25519(
				(remark_call.clone(), meta_tx_ext.clone(), meta_tx_ext.implicit().unwrap())
					.using_encoded(|e| alice_keyring.sign(&blake2_256(e))),
			);

			// Encode and share with the world.
			let meta_tx = (alice_account.clone(), remark_call, meta_tx_ext, meta_tx_sig).encode();

			// Bob acts as meta transaction relayer and constructs the transaction based on Alice
			// meta tx statement.

			type MetaTx = (AccountId, RuntimeCall, MetaTxExtension, Signature);
			let (signer, meta_tx_call, meta_tx_ext, meta_tx_sig) =
				MetaTx::decode(&mut &meta_tx[..]).unwrap();

			let signed_tx_ext = frame_support::transaction_extensions::VerifyAccountSignature::<
				MultiSignature,
			>::new_with_sign(meta_tx_sig, signer);

			let mut signed_tx_ext = signed_tx_ext.encode();
			signed_tx_ext.append(&mut meta_tx_ext.encode());

			let signed_tx_ext: SignedTxExtension =
				SignedTxExtension::decode(&mut &signed_tx_ext[..]).unwrap();

			// Bob signs the transaction with Alice's part to poof he is willing to sponsor the fee.
			let signed_tx_sign = MultiSignature::Sr25519(
				(meta_tx_call.clone(), signed_tx_ext.clone(), signed_tx_ext.implicit().unwrap())
					.using_encoded(|e| bob_keyring.sign(&blake2_256(e))),
			);

			let tx_ext = VerifyRelayerSignature::<MultiSignature>::new_with_relayer(
				signed_tx_sign,
				bob_account.clone(),
			);

			let mut tx_ext_encoded = tx_ext.encode();
			tx_ext_encoded.append(&mut signed_tx_ext.encode());

			let tx_ext: TxExtension = TxExtension::decode(&mut &tx_ext_encoded[..]).unwrap();

			let uxt = UncheckedExtrinsic::new_transaction(meta_tx_call, tx_ext);

			// Check Extrinsic validity and apply it.

			let uxt_info = uxt.get_dispatch_info();
			let uxt_len = uxt.using_encoded(|e| e.len());

			let xt = <UncheckedExtrinsic as Checkable<IdentityLookup<AccountId>>>::check(
				uxt,
				&Default::default(),
			)
			.unwrap();

			let res = xt.apply::<Runtime>(&uxt_info, uxt_len).unwrap();

			// Asserting the results.

			assert!(res.is_ok());

			System::assert_has_event(RuntimeEvent::System(frame_system::Event::Remarked {
				sender: alice_account.clone(),
				hash: <Runtime as frame_system::Config>::Hashing::hash(&[1]),
			}));

			// Alice balance is unchanged, Bob paid the transaction fee.
			assert_eq!(alice_balance, Balances::free_balance(alice_account));
			assert_eq!(bob_balance - tx_fee, Balances::free_balance(bob_account));
		});
	}
}
