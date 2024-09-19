// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
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

use crate::{Config, Key};
use codec::{Decode, Encode};
use core::{fmt, marker::PhantomData};
use frame_support::{dispatch::DispatchInfo, ensure};
use scale_info::TypeInfo;
use sp_runtime::{
	impl_tx_ext_default,
	traits::{AsSystemOriginSigner, DispatchInfoOf, Dispatchable, TransactionExtension},
	transaction_validity::{
		InvalidTransaction, TransactionPriority, TransactionValidityError, UnknownTransaction,
		ValidTransaction,
	},
};

/// Ensure that signed transactions are only valid if they are signed by sudo account.
///
/// In the initial phase of a chain without any tokens you can not prevent accounts from sending
/// transactions.
/// These transactions would enter the transaction pool as the succeed the validation, but would
/// fail on applying them as they are not allowed/disabled/whatever. This would be some huge dos
/// vector to any kind of chain. This extension solves the dos vector by preventing any kind of
/// transaction entering the pool as long as it is not signed by the sudo account.
#[derive(Clone, Eq, PartialEq, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct CheckOnlySudoAccount<T: Config + Send + Sync>(PhantomData<T>);

impl<T: Config + Send + Sync> Default for CheckOnlySudoAccount<T> {
	fn default() -> Self {
		Self(Default::default())
	}
}

impl<T: Config + Send + Sync> fmt::Debug for CheckOnlySudoAccount<T> {
	#[cfg(feature = "std")]
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "CheckOnlySudoAccount")
	}

	#[cfg(not(feature = "std"))]
	fn fmt(&self, _: &mut fmt::Formatter) -> fmt::Result {
		Ok(())
	}
}

impl<T: Config + Send + Sync> CheckOnlySudoAccount<T> {
	/// Creates new `TransactionExtension` to check sudo key.
	pub fn new() -> Self {
		Self::default()
	}
}

impl<T: Config + Send + Sync> TransactionExtension<<T as frame_system::Config>::RuntimeCall>
	for CheckOnlySudoAccount<T>
where
	<T as frame_system::Config>::RuntimeCall: Dispatchable<Info = DispatchInfo>,
	<<T as frame_system::Config>::RuntimeCall as Dispatchable>::RuntimeOrigin:
		AsSystemOriginSigner<T::AccountId> + Clone,
{
	const IDENTIFIER: &'static str = "CheckOnlySudoAccount";
	type Implicit = ();
	type Pre = ();
	type Val = ();

	fn weight(
		&self,
		_: &<T as frame_system::Config>::RuntimeCall,
	) -> frame_support::weights::Weight {
		use crate::weights::WeightInfo;
		T::WeightInfo::check_only_sudo_account()
	}

	fn validate(
		&self,
		origin: <<T as frame_system::Config>::RuntimeCall as Dispatchable>::RuntimeOrigin,
		_call: &<T as frame_system::Config>::RuntimeCall,
		info: &DispatchInfoOf<<T as frame_system::Config>::RuntimeCall>,
		_len: usize,
		_self_implicit: Self::Implicit,
		_inherited_implication: &impl Encode,
	) -> Result<
		(
			ValidTransaction,
			Self::Val,
			<<T as frame_system::Config>::RuntimeCall as Dispatchable>::RuntimeOrigin,
		),
		TransactionValidityError,
	> {
		let who = origin.as_system_origin_signer().ok_or(InvalidTransaction::BadSigner)?;
		let sudo_key: T::AccountId = Key::<T>::get().ok_or(UnknownTransaction::CannotLookup)?;
		ensure!(*who == sudo_key, InvalidTransaction::BadSigner);

		Ok((
			ValidTransaction {
				priority: info.total_weight().ref_time() as TransactionPriority,
				..Default::default()
			},
			(),
			origin,
		))
	}

	impl_tx_ext_default!(<T as frame_system::Config>::RuntimeCall; prepare);
}
