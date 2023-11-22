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

//! The [SimpleTransactionExtension] trait and related types.

use super::*;

/// Shortcut for the result value of the `validate` function.
pub type SimpleValidateResult<TE, Call> = Result<
	(ValidTransaction, <TE as SimpleTransactionExtension<Call>>::Val, OriginOf<Call>),
	TransactionValidityError,
>;

/// Means by which a transaction may be extended. This type embodies both the data and the logic
/// that should be additionally associated with the transaction. It should be plain old data.
///
/// This is slightly different to `TransactionExtension` owing to the fact that all associated
/// types are bound as `Default`. This means that default implementations can be provided for
/// all of the methods. If you impl using this type, you'll need to wrap it with `WithSimple`
/// when using it as a `TransactionExtension`.
pub trait SimpleTransactionExtension<Call: Dispatchable>:
	Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo
{
	/// Unique identifier of this signed extension.
	///
	/// This will be exposed in the metadata to identify the signed extension used
	/// in an extrinsic.
	const IDENTIFIER: &'static str;

	/// The type that encodes information that can be passed from validate to prepare.
	type Val: Default;

	/// The type that encodes information that can be passed from prepare to post-dispatch.
	type Pre: Default;

	/// Any additional data which was known at the time of transaction construction and
	/// can be useful in authenticating the transaction. This is determined dynamically in part
	/// from the on-chain environment using the `implied` function and not directly contained in
	/// the transction itself and therefore is considered "implicit".
	type Implicit: Encode + StaticTypeInfo + Default;

	/// Determine any additional data which was known at the time of transaction construction and
	/// can be useful in authenticating the transaction. The expected usage of this is to include
	/// in any data which is signed and verified as part of transactiob validation. Also perform
	/// any pre-signature-verification checks and return an error if needed.
	fn implicit(&self) -> Result<Self::Implicit, TransactionValidityError> {
		Ok(Default::default())
	}

	/// Validate a transaction for the transaction queue.
	///
	/// This function can be called frequently by the transaction queue to obtain transaction
	/// validity against current state. It should perform all checks that determine a valid
	/// transaction, that can pay for its execution and quickly eliminate ones that are stale or
	/// incorrect.
	fn validate(
		&self,
		origin: OriginOf<Call>,
		_call: &Call,
		_info: &DispatchInfoOf<Call>,
		_len: usize,
		_target: &[u8],
	) -> SimpleValidateResult<Self, Call> {
		Ok((Default::default(), Default::default(), origin))
	}

	/// Do any pre-flight stuff for a transaction after validation.
	///
	/// This is for actions which do not happen in the transaction queue but only immediately prior
	/// to the point of dispatch on-chain. This should not return an error, since errors
	/// should already have been identified during the [validate] call. If an error is returned,
	/// the transaction will be considered invalid.
	///
	/// Unlike `validate`, this function may consume `self`.
	///
	/// Checks made in validation need not be repeated here.
	fn prepare(
		self,
		_val: Self::Val,
		_origin: &OriginOf<Call>,
		_call: &Call,
		_info: &DispatchInfoOf<Call>,
		_len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		Ok(Default::default())
	}

	/// Do any post-flight stuff for an extrinsic.
	///
	/// `_pre` contains the output of `prepare`.
	///
	/// This gets given the `DispatchResult` `_result` from the extrinsic and can, if desired,
	/// introduce a `TransactionValidityError`, causing the block to become invalid for including
	/// it.
	///
	/// WARNING: It is dangerous to return an error here. To do so will fundamentally invalidate the
	/// transaction and any block that it is included in, causing the block author to not be
	/// compensated for their work in validating the transaction or producing the block so far.
	///
	/// It can only be used safely when you *know* that the extrinsic is one that can only be
	/// introduced by the current block author; generally this implies that it is an inherent and
	/// will come from either an offchain-worker or via `InherentData`.
	fn post_dispatch(
		_pre: Self::Pre,
		_info: &DispatchInfoOf<Call>,
		_post_info: &PostDispatchInfoOf<Call>,
		_len: usize,
		_result: &DispatchResult,
	) -> Result<(), TransactionValidityError> {
		Ok(())
	}

	/// Returns the metadata for this extension.
	///
	/// As a [`TransactionExtension`] can be a tuple of [`TransactionExtension`]s we need to return
	/// a `Vec` that holds the metadata of each one. Each individual `TransactionExtension` must
	/// return *exactly* one [`TransactionExtensionMetadata`].
	///
	/// This method provides a default implementation that returns a vec containing a single
	/// [`TransactionExtensionMetadata`].
	fn metadata() -> Vec<TransactionExtensionMetadata> {
		sp_std::vec![TransactionExtensionMetadata {
			identifier: Self::IDENTIFIER,
			ty: scale_info::meta_type::<Self>(),
			// TODO: Metadata-v16: Rename to "implicit"
			additional_signed: scale_info::meta_type::<Self::Implicit>()
		}]
	}
}

/// Transform a `SimpleTransactionExtension` into a `TransactionExtension`.
///
/// It will be transparent in so far as the metadata and encoding form is concerned, and you
/// can use `from`/`into`/`as_ref` to move between this type and its underlying
/// [SimpleTransactionExtension] instance.
#[derive(Encode, Decode, Default, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct WithSimple<S>(S);

impl<Call: Dispatchable, S: SimpleTransactionExtension<Call>> TransactionExtension<Call>
	for WithSimple<S>
{
	const IDENTIFIER: &'static str = S::IDENTIFIER;
	type Val = S::Val;
	type Pre = S::Pre;
	type Implicit = S::Implicit;
	fn implicit(&self) -> Result<Self::Implicit, TransactionValidityError> {
		self.0.implicit()
	}
	fn validate(
		&self,
		origin: OriginOf<Call>,
		call: &Call,
		info: &DispatchInfoOf<Call>,
		len: usize,
		target: &[u8],
	) -> ValidateResult<Self, Call> {
		self.0.validate(origin, call, info, len, target)
	}
	fn prepare(
		self,
		val: Self::Val,
		origin: &OriginOf<Call>,
		call: &Call,
		info: &DispatchInfoOf<Call>,
		len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		self.0.prepare(val, origin, call, info, len)
	}
	fn post_dispatch(
		pre: Self::Pre,
		info: &DispatchInfoOf<Call>,
		post_info: &PostDispatchInfoOf<Call>,
		len: usize,
		result: &DispatchResult,
	) -> Result<(), TransactionValidityError> {
		S::post_dispatch(pre, info, post_info, len, result)
	}
	fn metadata() -> Vec<TransactionExtensionMetadata> {
		S::metadata()
	}
}

impl<S: TypeInfo> TypeInfo for WithSimple<S> {
	type Identity = S::Identity;
	fn type_info() -> Type {
		S::type_info()
	}
}

impl<S> From<S> for WithSimple<S> {
	fn from(value: S) -> Self {
		Self(value)
	}
}

impl<S> AsRef<S> for WithSimple<S> {
	fn as_ref(&self) -> &S {
		&self.0
	}
}

impl<S> AsMut<S> for WithSimple<S> {
	fn as_mut(&mut self) -> &mut S {
		&mut self.0
	}
}
