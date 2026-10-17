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

//! Contains transaction extensions needed for ethereum compatability.

use crate::{CallOf, Config, Origin, OriginFor, SubstrateTxSigner, WeightInfo};
use codec::{Decode, DecodeWithMemTracking, Encode};
use frame_support::{
	DebugNoBound, DefaultNoBound,
	pallet_prelude::{InvalidTransaction, TransactionSource},
	traits::OriginTrait,
};
use scale_info::TypeInfo;
use sp_runtime::{
	DispatchResult, Weight,
	traits::{DispatchInfoOf, PostDispatchInfoOf, TransactionExtension, ValidateResult},
	transaction_validity::TransactionValidityError,
};

/// An extension that sets the origin to [`Origin::EthTransaction`] in case it originated from an
/// eth transaction.
///
/// For a signed substrate transaction it records the signer in `SubstrateTxSigner` for the
/// duration of the dispatch. The pallet needs it to tell whether a signed origin had its nonce
/// consumed by `CheckNonce`.
///
/// This extension needs to be put behind any other extension that relies on a signed origin.
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	Clone,
	Eq,
	PartialEq,
	DefaultNoBound,
	TypeInfo,
	DebugNoBound,
)]
#[scale_info(skip_type_params(T))]
pub struct SetOrigin<T: Config + Send + Sync> {
	/// Skipped as can only be set by runtime code.
	#[codec(skip)]
	is_eth_transaction: bool,
	_phantom: core::marker::PhantomData<T>,
}

impl<T: Config + Send + Sync> SetOrigin<T> {
	/// Create the extension so that it will transform the origin.
	///
	/// If the extension is default constructed it will do nothing.
	pub fn new_from_eth_transaction() -> Self {
		Self { is_eth_transaction: true, _phantom: Default::default() }
	}
}

impl<T> TransactionExtension<CallOf<T>> for SetOrigin<T>
where
	T: Config + Send + Sync,
	OriginFor<T>: From<Origin<T>>,
{
	const IDENTIFIER: &'static str = "EthSetOrigin";
	type Implicit = ();
	/// Whether the signer was recorded and needs to be removed after dispatch.
	type Pre = bool;
	type Val = ();

	fn weight(&self, _: &CallOf<T>) -> Weight {
		if self.is_eth_transaction {
			Weight::zero()
		} else {
			T::WeightInfo::set_origin_substrate_tx()
		}
	}

	fn validate(
		&self,
		origin: OriginFor<T>,
		_call: &CallOf<T>,
		_info: &DispatchInfoOf<CallOf<T>>,
		_len: usize,
		_self_implicit: Self::Implicit,
		_inherited_implication: &impl Encode,
		_source: TransactionSource,
	) -> ValidateResult<Self::Val, CallOf<T>> {
		let origin = if self.is_eth_transaction {
			let signer =
				frame_system::ensure_signed(origin).map_err(|_| InvalidTransaction::BadProof)?;
			Origin::EthTransaction(signer).into()
		} else {
			origin
		};
		Ok((Default::default(), Default::default(), origin))
	}

	fn prepare(
		self,
		_val: Self::Val,
		origin: &OriginFor<T>,
		_call: &CallOf<T>,
		_info: &DispatchInfoOf<CallOf<T>>,
		_len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		if self.is_eth_transaction {
			return Ok(false);
		}
		let Some(signer) = origin.as_signer() else { return Ok(false) };
		SubstrateTxSigner::<T>::put(signer);
		Ok(true)
	}

	fn post_dispatch_details(
		pre: Self::Pre,
		_info: &DispatchInfoOf<CallOf<T>>,
		_post_info: &PostDispatchInfoOf<CallOf<T>>,
		_len: usize,
		_result: &DispatchResult,
	) -> Result<Weight, TransactionValidityError> {
		if pre {
			SubstrateTxSigner::<T>::kill();
			Ok(Weight::zero())
		} else {
			Ok(T::WeightInfo::set_origin_substrate_tx())
		}
	}
}
