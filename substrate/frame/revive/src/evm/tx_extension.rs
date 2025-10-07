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

use crate::{CallOf, Config, Origin, OriginFor};
use codec::{Decode, DecodeWithMemTracking, Encode};
use frame_support::{
	pallet_prelude::{InvalidTransaction, TransactionSource},
	DebugNoBound, DefaultNoBound,
};
use scale_info::TypeInfo;
use sp_runtime::{
	impl_tx_ext_default,
	traits::{DispatchInfoOf, TransactionExtension, ValidateResult},
	Weight,
};

/// An extension that sets the origin to [`Origin::EthTransaction`] in case it originated from an
/// eth transaction.
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
	type Pre = ();
	type Val = ();

	fn weight(&self, _: &CallOf<T>) -> Weight {
		Default::default()
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

	impl_tx_ext_default!(CallOf<T>; prepare);
}
