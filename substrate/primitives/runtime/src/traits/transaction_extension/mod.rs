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

//! The transaction extension trait.

use crate::{
	scale_info::{MetaType, StaticTypeInfo},
	transaction_validity::{TransactionValidity, TransactionValidityError, ValidTransaction},
	DispatchResult,
};
use codec::{Codec, Decode, Encode};
use impl_trait_for_tuples::impl_for_tuples;
#[doc(hidden)]
pub use sp_std::marker::PhantomData;
use sp_std::{self, fmt::Debug, prelude::*};
use sp_weights::Weight;
use tuplex::{PopFront, PushBack};

use super::{DispatchInfoOf, Dispatchable, OriginOf, PostDispatchInfoOf};

mod as_transaction_extension;
mod dispatch_transaction;
#[allow(deprecated)]
pub use as_transaction_extension::AsTransactionExtension;
pub use dispatch_transaction::DispatchTransaction;

/// Shortcut for the result value of the `validate` function.
pub type ValidateResult<Val, Call> =
	Result<(ValidTransaction, Val, OriginOf<Call>), TransactionValidityError>;

/// Simple blanket implementation trait to denote the bounds of a type which can be contained within
/// a [`TransactionExtension`].
pub trait TransactionExtensionInterior:
	Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo
{
}
impl<T: Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo>
	TransactionExtensionInterior for T
{
}

/// Base for [TransactionExtension]s; this contains the associated types and does not require any
/// generic parameterization.
pub trait TransactionExtensionBase: TransactionExtensionInterior {
	/// Unique identifier of this signed extension.
	///
	/// This will be exposed in the metadata to identify the signed extension used in an extrinsic.
	const IDENTIFIER: &'static str;

	/// Any additional data which was known at the time of transaction construction and can be
	/// useful in authenticating the transaction. This is determined dynamically in part from the
	/// on-chain environment using the `implicit` function and not directly contained in the
	/// transaction itself and therefore is considered "implicit".
	type Implicit: Codec + StaticTypeInfo;

	/// Determine any additional data which was known at the time of transaction construction and
	/// can be useful in authenticating the transaction. The expected usage of this is to include in
	/// any data which is signed and verified as part of transactiob validation. Also perform any
	/// pre-signature-verification checks and return an error if needed.
	fn implicit(&self) -> Result<Self::Implicit, TransactionValidityError> {
		use crate::InvalidTransaction::IndeterminateImplicit;
		Ok(Self::Implicit::decode(&mut &[][..]).map_err(|_| IndeterminateImplicit)?)
	}

	/// The weight consumed by executing this extension instance fully during transaction dispatch.
	fn weight(&self) -> Weight {
		Weight::zero()
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

/// Means by which a transaction may be extended. This type embodies both the data and the logic
/// that should be additionally associated with the transaction. It should be plain old data.
///
/// The simplest transaction extension would be the Unit type (and empty pipeline) `()`. This
/// executes no additional logic and implies a dispatch of the transaction's call using the
/// inherited origin (either `None` or `Signed`, depending on whether this is a signed or general
/// transaction).
///
/// Transaction extensions are capable of altering certain associated semantics:
///
/// - They may define the origin with which the transaction's call should be dispatched.
/// - They may define various parameters used by the transction queue to determine under what
///   conditions the transaction should be retained and introduced on-chain.
/// - They may define whether this transaction is acceptable for introduction on-chain at all.
///
/// Each of these semantics are defined by the `validate` function.
///
/// **NOTE: Transaction extensions cannot under any circumctances alter the call itself.**
///
/// Transaction extensions are capable of defining logic which is executed additionally to the
/// dispatch of the call:
///
/// - They may define logic which must be executed prior to the dispatch of the call.
/// - They may also define logic which must be executed after the dispatch of the call.
///
/// Each of these semantics are defined by the `prepare` and `post_dispatch` functions respectively.
///
/// Finally, transaction extensions may define additional data to help define the implications of
/// the logic they introduce. This additional data may be explicitly defined by the transaction
/// author (in which case it is included as part of the transaction body), or it may be implicitly
/// defined by the transaction extension based around the on-chain state (which the transaction
/// author is assumed to know). This data may be utilized by the above logic to alter how a node's
/// transaction queue treats this transaction.
///
/// ## Default implementations
///
/// Of the 5 functions in this trait, 3 of them must return a value of an associated type on
/// success, and none of these types implement [Default] or anything like it. This means that
/// default implementations cannot be provided for these functions. However, a macro is provided
/// [impl_tx_ext_default](crate::impl_tx_ext_default) which is capable of generating default
/// implementations for each of these 3 functions. If you do not wish to introduce additional logic
/// into the transaction pipeline, then it is recommended that you use this macro to implement these
/// functions.
///
/// ## Pipelines, Inherited Implications, and Authorized Origins
///
/// Requiring a single transaction extension to define all of the above semantics would be
/// cumbersome and would lead to a lot of boilerplate. Instead, transaction extensions are
/// aggregated into pipelines, which are tuples of transaction extensions. Each extension in the
/// pipeline is executed in order, and the output of each extension is aggregated and/or relayed as
/// the input to the next extension in the pipeline.
///
/// This ordered composition happens with all datatypes ([Val](TransactionExtension::Val),
/// [Pre](TransactionExtension::Pre) and [Implicit](TransactionExtensionBase::Implicit)) as well as
/// all functions. There are important consequences stemming from how the composition affects the
/// meaning of the `origin` and `implication` parameters as well as the results. Whereas the
/// [prepare](TransactionExtension::prepare) and
/// [post_dispatch](TransactionExtension::post_dispatch) functions are clear in their meaning, the
/// [validate](TransactionExtension::validate) function is fairly sophisticated and warrants
/// further explanation.
///
/// Firstly, the `origin` parameter. The `origin` passed into the first item in a pipeline is simply
/// that passed into the tuple itself. It represents an authority who has authorized the implication
/// of the transaction, as of the extension it has been passed into *and any further extensions it
/// may pass though, all the way to, and including, the transaction's dispatch call itself. Each
/// following item in the pipeline is passed the origin which the previous item returned. The origin
/// returned from the final item in the pipeline is the origin which is returned by the tuple
/// itself.
///
/// This means that if a constituent extension returns a different origin to the one it was called
/// with, then (assuming no other extension changes it further) *this new origin will be used for
/// all extensions following it in the pipeline, and will be returned from the pipeline to be used
/// as the origin for the call's dispatch*. The call itself as well as all these extensions
/// following may each imply consequence for this origin. We call this the *inherited implication*.
///
/// The *inherited implication* is the cumulated on-chain effects born by whatever origin is
/// returned. It is expressed to the [validate](TransactionExtension::validate) function only as the
/// `implication` argument which implements the [Encode] trait. A transaction extension may define
/// its own implications through its own fields and the
/// [implicit](TransactionExtensionBase::implicit) function. This is only utilized by extensions
/// which preceed it in a pipeline or, if the transaction is an old-school signed trasnaction, the
/// underlying transaction verification logic.
///
/// **The inherited implication passed as the `implication` parameter to
/// [validate](TransactionExtension::validate) does not include the extension's inner data itself
/// nor does it include the result of the extension's `implicit` function.** If you both provide an
/// implication and rely on the implication, then you need to manually aggregate your extensions
/// implication with the aggregated implication passed in.
pub trait TransactionExtension<Call: Dispatchable, Context>: TransactionExtensionBase {
	/// The type that encodes information that can be passed from validate to prepare.
	type Val;

	/// The type that encodes information that can be passed from prepare to post-dispatch.
	type Pre;

	/// Validate a transaction for the transaction queue.
	///
	/// This function can be called frequently by the transaction queue to obtain transaction
	/// validity against current state. It should perform all checks that determine a valid
	/// transaction, that can pay for its execution and quickly eliminate ones that are stale or
	/// incorrect.
	///
	/// Parameters:
	/// - `origin`: The origin of the transaction which this extension inherited; coming from an
	///   "old-school" *signed transaction*, this will be a system `RawOrigin::Signed` value. If the
	///   transaction is a "new-school" *General Transaction*, then this will be a system
	///   `RawOrigin::None` value. If this extension is an item in a composite, then it could be
	///   anything which was previously returned as an `origin` value in the result of a `validate`
	///   call.
	/// - `call`: The `Call` wrapped by this extension.
	/// - `info`: Information concerning, and inherent to, the transaction's call.
	/// - `len`: The total length of the encoded transaction.
	/// - `inherited_implication`: The *implication* which this extension inherits. This is a tuple
	///   of the transaction's call and some additional opaque-but-encodable data. Coming directly
	///   from a transaction, the latter is [()]. However, if this extension is expressed as part of
	///   a composite type, then the latter component is equal to any further implications to which
	///   the returned `origin` could potentially apply. See Pipelines, Inherited Implications, and
	///   Authorized Origins for more information.
	/// - `context`: Some opaque mutable context, as yet unused.
	///
	/// Returns a [ValidateResult], which is a [Result] whose success type is a tuple of
	/// [ValidTransaction] (defining useful metadata for the transaction queue), the [Self::Val]
	/// token of this transaction, which gets passed into [prepare](TransactionExtension::prepare),
	/// and the origin of the transaction, which gets passed into
	/// [prepare](TransactionExtension::prepare) and is ultimately used for dispatch.
	fn validate(
		&self,
		origin: OriginOf<Call>,
		call: &Call,
		info: &DispatchInfoOf<Call>,
		len: usize,
		context: &mut Context,
		self_implicit: Self::Implicit,
		inherited_implication: &impl Encode,
	) -> ValidateResult<Self::Val, Call>;

	/// Do any pre-flight stuff for a transaction after validation.
	///
	/// This is for actions which do not happen in the transaction queue but only immediately prior
	/// to the point of dispatch on-chain. This should not return an error, since errors should
	/// already have been identified during the [validate](TransactionExtension::validate) call. If
	/// an error is returned, the transaction will be considered invalid but no state changes will
	/// happen and therefore work done in [validate](TransactionExtension::validate) will not be
	/// paid for.
	///
	/// Unlike `validate`, this function may consume `self`.
	///
	/// Parameters:
	/// - `val`: `Self::Val` returned by the result of the `validate` call.
	/// - `origin`: The origin returned by the result of the `validate` call.
	/// - `call`: The `Call` wrapped by this extension.
	/// - `info`: Information concerning, and inherent to, the transaction's call.
	/// - `len`: The total length of the encoded transaction.
	/// - `context`: Some opaque mutable context, as yet unused.
	///
	/// Returns a [Self::Pre] value on success, which gets passed into
	/// [post_dispatch](TransactionExtension::post_dispatch) and after the call is dispatched.
	///
	/// IMPORTANT: **Checks made in validation need not be repeated here.**
	fn prepare(
		self,
		val: Self::Val,
		origin: &OriginOf<Call>,
		call: &Call,
		info: &DispatchInfoOf<Call>,
		len: usize,
		context: &Context,
	) -> Result<Self::Pre, TransactionValidityError>;

	/// Do any post-flight stuff for an extrinsic.
	///
	/// `_pre` contains the output of `prepare`.
	///
	/// This gets given the `DispatchResult` `_result` from the extrinsic and can, if desired,
	/// introduce a `TransactionValidityError`, causing the block to become invalid for including
	/// it.
	///
	/// Parameters:
	/// - `pre`: `Self::Pre` returned by the result of the `prepare` call prior to dispatch.
	/// - `info`: Information concerning, and inherent to, the transaction's call.
	/// - `post_info`: Information concerning the dispatch of the transaction's call.
	/// - `len`: The total length of the encoded transaction.
	/// - `result`: The result of the dispatch.
	/// - `context`: Some opaque mutable context, as yet unused.
	///
	/// WARNING: It is dangerous to return an error here. To do so will fundamentally invalidate the
	/// transaction and any block that it is included in, causing the block author to not be
	/// compensated for their work in validating the transaction or producing the block so far. It
	/// can only be used safely when you *know* that the transaction is one that would only be
	/// introduced by the current block author.
	fn post_dispatch(
		_pre: Self::Pre,
		_info: &DispatchInfoOf<Call>,
		_post_info: &PostDispatchInfoOf<Call>,
		_len: usize,
		_result: &DispatchResult,
		_context: &Context,
	) -> Result<(), TransactionValidityError> {
		Ok(())
	}

	/// Compatibility function for supporting the `SignedExtension::validate_unsigned` function.
	///
	/// DO NOT USE! THIS MAY BE REMOVED AT ANY TIME!
	#[deprecated = "Only for compatibility. DO NOT USE."]
	fn validate_bare_compat(
		_call: &Call,
		_info: &DispatchInfoOf<Call>,
		_len: usize,
	) -> TransactionValidity {
		Ok(ValidTransaction::default())
	}

	/// Compatibility function for supporting the `SignedExtension::pre_dispatch_unsigned` function.
	///
	/// DO NOT USE! THIS MAY BE REMOVED AT ANY TIME!
	#[deprecated = "Only for compatibility. DO NOT USE."]
	fn pre_dispatch_bare_compat(
		_call: &Call,
		_info: &DispatchInfoOf<Call>,
		_len: usize,
	) -> Result<(), TransactionValidityError> {
		Ok(())
	}

	/// Compatibility function for supporting the `SignedExtension::post_dispatch` function where
	/// `pre` is `None`.
	///
	/// DO NOT USE! THIS MAY BE REMOVED AT ANY TIME!
	#[deprecated = "Only for compatibility. DO NOT USE."]
	fn post_dispatch_bare_compat(
		_info: &DispatchInfoOf<Call>,
		_post_info: &PostDispatchInfoOf<Call>,
		_len: usize,
		_result: &DispatchResult,
	) -> Result<(), TransactionValidityError> {
		Ok(())
	}
}

/// Implict
#[macro_export]
macro_rules! impl_tx_ext_default {
	($call:ty ; $context:ty ; , $( $rest:tt )*) => {
		impl_tx_ext_default!{$call ; $context ; $( $rest )*}
	};
	($call:ty ; $context:ty ; validate $( $rest:tt )*) => {
		fn validate(
			&self,
			origin: $crate::traits::OriginOf<$call>,
			_call: &$call,
			_info: &$crate::traits::DispatchInfoOf<$call>,
			_len: usize,
			_context: &mut $context,
			_self_implicit: Self::Implicit,
			_inherited_implication: &impl $crate::codec::Encode,
		) -> $crate::traits::ValidateResult<Self::Val, $call> {
			Ok((Default::default(), Default::default(), origin))
		}
		impl_tx_ext_default!{$call ; $context ; $( $rest )*}
	};
	($call:ty ; $context:ty ; prepare $( $rest:tt )*) => {
		fn prepare(
			self,
			_val: Self::Val,
			_origin: &$crate::traits::OriginOf<$call>,
			_call: &$call,
			_info: &$crate::traits::DispatchInfoOf<$call>,
			_len: usize,
			_context: & $context,
		) -> Result<Self::Pre, $crate::TransactionValidityError> {
			Ok(Default::default())
		}
		impl_tx_ext_default!{$call ; $context ; $( $rest )*}
	};
	($call:ty ; $context:ty ;) => {};
}

/// Information about a [`TransactionExtension`] for the runtime metadata.
pub struct TransactionExtensionMetadata {
	/// The unique identifier of the [`TransactionExtension`].
	pub identifier: &'static str,
	/// The type of the [`TransactionExtension`].
	pub ty: MetaType,
	/// The type of the [`TransactionExtension`] additional signed data for the payload.
	// TODO: Rename "implicit"
	pub additional_signed: MetaType,
}

#[impl_for_tuples(1, 12)]
impl TransactionExtensionBase for Tuple {
	for_tuples!( where #( Tuple: TransactionExtensionBase )* );
	const IDENTIFIER: &'static str = "Use `metadata()`!";
	for_tuples!( type Implicit = ( #( Tuple::Implicit ),* ); );
	fn implicit(&self) -> Result<Self::Implicit, TransactionValidityError> {
		Ok(for_tuples!( ( #( Tuple.implicit()? ),* ) ))
	}
	fn weight(&self) -> Weight {
		let mut weight = Weight::zero();
		for_tuples!( #( weight += Tuple.weight(); )* );
		weight
	}
	fn metadata() -> Vec<TransactionExtensionMetadata> {
		let mut ids = Vec::new();
		for_tuples!( #( ids.extend(Tuple::metadata()); )* );
		ids
	}
}

#[impl_for_tuples(1, 12)]
impl<Call: Dispatchable, Context> TransactionExtension<Call, Context> for Tuple {
	for_tuples!( where #( Tuple: TransactionExtension<Call, Context> )* );
	for_tuples!( type Val = ( #( Tuple::Val ),* ); );
	for_tuples!( type Pre = ( #( Tuple::Pre ),* ); );

	fn validate(
		&self,
		origin: <Call as Dispatchable>::RuntimeOrigin,
		call: &Call,
		info: &DispatchInfoOf<Call>,
		len: usize,
		context: &mut Context,
		self_implicit: Self::Implicit,
		inherited_implication: &impl Encode,
	) -> Result<
		(ValidTransaction, Self::Val, <Call as Dispatchable>::RuntimeOrigin),
		TransactionValidityError,
	> {
		let valid = ValidTransaction::default();
		let val = ();
		let following_explicit_implications = for_tuples!( ( #( &self.Tuple ),* ) );
		let following_implicit_implications = self_implicit;

		for_tuples!(#(
			// Implication of this pipeline element not relevant for later items, so we pop it.
			let (_item, following_explicit_implications) = following_explicit_implications.pop_front();
			let (item_implicit, following_implicit_implications) = following_implicit_implications.pop_front();
			let (item_valid, item_val, origin) = {
				let implications = (
					// The first is the implications born of the fact we return the mutated
					// origin.
					inherited_implication,
					// This is the explicitly made implication born of the fact the new origin is
					// passed into the next items in this pipeline-tuple.
					&following_explicit_implications,
					// This is the implicitly made implication born of the fact the new origin is
					// passed into the next items in this pipeline-tuple.
					&following_implicit_implications,
				);
				Tuple.validate(origin, call, info, len, context, item_implicit, &implications)?
			};
			let valid = valid.combine_with(item_valid);
			let val = val.push_back(item_val);
		)* );
		Ok((valid, val, origin))
	}

	fn prepare(
		self,
		val: Self::Val,
		origin: &<Call as Dispatchable>::RuntimeOrigin,
		call: &Call,
		info: &DispatchInfoOf<Call>,
		len: usize,
		context: &Context,
	) -> Result<Self::Pre, TransactionValidityError> {
		Ok(for_tuples!( ( #(
			Tuple::prepare(self.Tuple, val.Tuple, origin, call, info, len, context)?
		),* ) ))
	}

	fn post_dispatch(
		pre: Self::Pre,
		info: &DispatchInfoOf<Call>,
		post_info: &PostDispatchInfoOf<Call>,
		len: usize,
		result: &DispatchResult,
		context: &Context,
	) -> Result<(), TransactionValidityError> {
		for_tuples!( #( Tuple::post_dispatch(pre.Tuple, info, post_info, len, result, context)?; )* );
		Ok(())
	}
}

impl TransactionExtensionBase for () {
	const IDENTIFIER: &'static str = "UnitTransactionExtension";
	type Implicit = ();
	fn implicit(&self) -> sp_std::result::Result<Self::Implicit, TransactionValidityError> {
		Ok(())
	}
	fn weight(&self) -> Weight {
		Weight::zero()
	}
}

impl<Call: Dispatchable, Context> TransactionExtension<Call, Context> for () {
	type Val = ();
	type Pre = ();
	fn validate(
		&self,
		origin: <Call as Dispatchable>::RuntimeOrigin,
		_call: &Call,
		_info: &DispatchInfoOf<Call>,
		_len: usize,
		_context: &mut Context,
		_self_implicit: Self::Implicit,
		_inherited_implication: &impl Encode,
	) -> Result<
		(ValidTransaction, (), <Call as Dispatchable>::RuntimeOrigin),
		TransactionValidityError,
	> {
		Ok((ValidTransaction::default(), (), origin))
	}
	fn prepare(
		self,
		_val: (),
		_origin: &<Call as Dispatchable>::RuntimeOrigin,
		_call: &Call,
		_info: &DispatchInfoOf<Call>,
		_len: usize,
		_context: &Context,
	) -> Result<(), TransactionValidityError> {
		Ok(())
	}
}
