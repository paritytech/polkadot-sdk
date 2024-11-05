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

use super::{
	DispatchInfoOf, DispatchOriginOf, Dispatchable, ExtensionPostDispatchWeightHandler,
	PostDispatchInfoOf, RefundWeight,
};

mod as_transaction_extension;
mod dispatch_transaction;
#[allow(deprecated)]
pub use as_transaction_extension::AsTransactionExtension;
pub use dispatch_transaction::DispatchTransaction;

/// Shortcut for the result value of the `validate` function.
pub type ValidateResult<Val, Call> =
	Result<(ValidTransaction, Val, DispatchOriginOf<Call>), TransactionValidityError>;

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
/// - They may define various parameters used by the transaction queue to determine under what
///   conditions the transaction should be retained and introduced on-chain.
/// - They may define whether this transaction is acceptable for introduction on-chain at all.
///
/// Each of these semantics are defined by the `validate` function.
///
/// **NOTE: Transaction extensions cannot under any circumstances alter the call itself.**
///
/// Transaction extensions are capable of defining logic which is executed additionally to the
/// dispatch of the call:
///
/// - They may define logic which must be executed prior to the dispatch of the call.
/// - They may also define logic which must be executed after the dispatch of the call.
///
/// Each of these semantics are defined by the `prepare` and `post_dispatch_details` functions
/// respectively.
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
/// Of the 6 functions in this trait along with `TransactionExtension`, 2 of them must return a
/// value of an associated type on success, with only `implicit` having a default implementation.
/// This means that default implementations cannot be provided for `validate` and `prepare`.
/// However, a macro is provided [impl_tx_ext_default](crate::impl_tx_ext_default) which is capable
/// of generating default implementations for both of these functions. If you do not wish to
/// introduce additional logic into the transaction pipeline, then it is recommended that you use
/// this macro to implement these functions. Additionally, [weight](TransactionExtension::weight)
/// can return a default value, which would mean the extension is weightless, but it is not
/// implemented by default. Instead, implementers can explicitly choose to implement this default
/// behavior through the same [impl_tx_ext_default](crate::impl_tx_ext_default) macro.
///
/// If your extension does any post-flight logic, then the functionality must be implemented in
/// [post_dispatch_details](TransactionExtension::post_dispatch_details). This function can return
/// the actual weight used by the extension during an entire dispatch cycle by wrapping said weight
/// value in a `Some`. This is useful in computing fee refunds, similar to how post dispatch
/// information is used to refund fees for calls. Alternatively, a `None` can be returned, which
/// means that the worst case scenario weight, namely the value returned by
/// [weight](TransactionExtension::weight), is the actual weight. This particular piece of logic
/// is embedded in the default implementation of
/// [post_dispatch](TransactionExtension::post_dispatch) so that the weight is assumed to be worst
/// case scenario, but implementers of this trait can correct it with extra effort. Therefore, all
/// users of an extension should use [post_dispatch](TransactionExtension::post_dispatch), with
/// [post_dispatch_details](TransactionExtension::post_dispatch_details) considered an internal
/// function.
///
/// ## Pipelines, Inherited Implications, and Authorized Origins
///
/// Requiring a single transaction extension to define all of the above semantics would be
/// cumbersome and would lead to a lot of boilerplate. Instead, transaction extensions are
/// aggregated into pipelines, which are tuples of transaction extensions. Each extension in the
/// pipeline is executed in order, and the output of each extension is aggregated and/or relayed as
/// the input to the next extension in the pipeline.
///
/// This ordered composition happens with all data types ([Val](TransactionExtension::Val),
/// [Pre](TransactionExtension::Pre) and [Implicit](TransactionExtension::Implicit)) as well as
/// all functions. There are important consequences stemming from how the composition affects the
/// meaning of the `origin` and `implication` parameters as well as the results. Whereas the
/// [prepare](TransactionExtension::prepare) and
/// [post_dispatch](TransactionExtension::post_dispatch) functions are clear in their meaning, the
/// [validate](TransactionExtension::validate) function is fairly sophisticated and warrants further
/// explanation.
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
/// [implicit](TransactionExtension::implicit) function. This is only utilized by extensions
/// which precede it in a pipeline or, if the transaction is an old-school signed transaction, the
/// underlying transaction verification logic.
///
/// **The inherited implication passed as the `implication` parameter to
/// [validate](TransactionExtension::validate) does not include the extension's inner data itself
/// nor does it include the result of the extension's `implicit` function.** If you both provide an
/// implication and rely on the implication, then you need to manually aggregate your extensions
/// implication with the aggregated implication passed in.
///
/// In the post dispatch pipeline, the actual weight of each extension is accrued in the
/// [PostDispatchInfo](PostDispatchInfoOf<Call>) of that transaction sequentially with each
/// [post_dispatch](TransactionExtension::post_dispatch) call. This means that an extension handling
/// transaction payment and refunds should be at the end of the pipeline in order to capture the
/// correct amount of weight used during the call. This is because one cannot know the actual weight
/// of an extension after post dispatch without running the post dispatch ahead of time.
pub trait TransactionExtension<Call: Dispatchable>:
	Codec + Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo
{
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
	/// any data which is signed and verified as part of transaction validation. Also perform any
	/// pre-signature-verification checks and return an error if needed.
	fn implicit(&self) -> Result<Self::Implicit, TransactionValidityError> {
		use crate::transaction_validity::InvalidTransaction::IndeterminateImplicit;
		Ok(Self::Implicit::decode(&mut &[][..]).map_err(|_| IndeterminateImplicit)?)
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
			implicit: scale_info::meta_type::<Self::Implicit>()
		}]
	}

	/// The type that encodes information that can be passed from `validate` to `prepare`.
	type Val;

	/// The type that encodes information that can be passed from `prepare` to `post_dispatch`.
	type Pre;

	/// The weight consumed by executing this extension instance fully during transaction dispatch.
	fn weight(&self, call: &Call) -> Weight;

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
	///
	/// Returns a [ValidateResult], which is a [Result] whose success type is a tuple of
	/// [ValidTransaction] (defining useful metadata for the transaction queue), the [Self::Val]
	/// token of this transaction, which gets passed into [prepare](TransactionExtension::prepare),
	/// and the origin of the transaction, which gets passed into
	/// [prepare](TransactionExtension::prepare) and is ultimately used for dispatch.
	fn validate(
		&self,
		origin: DispatchOriginOf<Call>,
		call: &Call,
		info: &DispatchInfoOf<Call>,
		len: usize,
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
	///
	/// Returns a [Self::Pre] value on success, which gets passed into
	/// [post_dispatch](TransactionExtension::post_dispatch) and after the call is dispatched.
	///
	/// IMPORTANT: **Checks made in validation need not be repeated here.**
	fn prepare(
		self,
		val: Self::Val,
		origin: &DispatchOriginOf<Call>,
		call: &Call,
		info: &DispatchInfoOf<Call>,
		len: usize,
	) -> Result<Self::Pre, TransactionValidityError>;

	/// Do any post-flight stuff for an extrinsic.
	///
	/// `_pre` contains the output of `prepare`.
	///
	/// This gets given the `DispatchResult` `_result` from the extrinsic and can, if desired,
	/// introduce a `TransactionValidityError`, causing the block to become invalid for including
	/// it.
	///
	/// On success, the caller must return the amount of unspent weight left over by this extension
	/// after dispatch. By default, this function returns no unspent weight, which means the entire
	/// weight computed for the worst case scenario is consumed.
	///
	/// WARNING: This function does not automatically keep track of accumulated "actual" weight.
	/// Unless this weight is handled at the call site, use
	/// [post_dispatch](TransactionExtension::post_dispatch)
	/// instead.
	///
	/// Parameters:
	/// - `pre`: `Self::Pre` returned by the result of the `prepare` call prior to dispatch.
	/// - `info`: Information concerning, and inherent to, the transaction's call.
	/// - `post_info`: Information concerning the dispatch of the transaction's call.
	/// - `len`: The total length of the encoded transaction.
	/// - `result`: The result of the dispatch.
	///
	/// WARNING: It is dangerous to return an error here. To do so will fundamentally invalidate the
	/// transaction and any block that it is included in, causing the block author to not be
	/// compensated for their work in validating the transaction or producing the block so far. It
	/// can only be used safely when you *know* that the transaction is one that would only be
	/// introduced by the current block author.
	fn post_dispatch_details(
		_pre: Self::Pre,
		_info: &DispatchInfoOf<Call>,
		_post_info: &PostDispatchInfoOf<Call>,
		_len: usize,
		_result: &DispatchResult,
	) -> Result<Weight, TransactionValidityError> {
		Ok(Weight::zero())
	}

	/// A wrapper for [`post_dispatch_details`](TransactionExtension::post_dispatch_details) that
	/// refunds the unspent weight consumed by this extension into the post dispatch information.
	///
	/// If `post_dispatch_details` returns a non-zero unspent weight, which, by definition, must be
	/// less than the worst case weight provided by [weight](TransactionExtension::weight), that
	/// is the value refunded in `post_info`.
	///
	/// If no unspent weight is reported by `post_dispatch_details`, this function assumes the worst
	/// case weight and does not refund anything.
	///
	/// For more information, look into
	/// [post_dispatch_details](TransactionExtension::post_dispatch_details).
	fn post_dispatch(
		pre: Self::Pre,
		info: &DispatchInfoOf<Call>,
		post_info: &mut PostDispatchInfoOf<Call>,
		len: usize,
		result: &DispatchResult,
	) -> Result<(), TransactionValidityError> {
		let unspent_weight = Self::post_dispatch_details(pre, info, &post_info, len, result)?;
		post_info.refund(unspent_weight);

		Ok(())
	}

	/// Validation logic for bare extrinsics.
	///
	/// NOTE: This function will be migrated to a separate `InherentExtension` interface.
	fn bare_validate(
		_call: &Call,
		_info: &DispatchInfoOf<Call>,
		_len: usize,
	) -> TransactionValidity {
		Ok(ValidTransaction::default())
	}

	/// All pre-flight logic run before dispatching bare extrinsics.
	///
	/// NOTE: This function will be migrated to a separate `InherentExtension` interface.
	fn bare_validate_and_prepare(
		_call: &Call,
		_info: &DispatchInfoOf<Call>,
		_len: usize,
	) -> Result<(), TransactionValidityError> {
		Ok(())
	}

	/// Post dispatch logic run after dispatching bare extrinsics.
	///
	/// NOTE: This function will be migrated to a separate `InherentExtension` interface.
	fn bare_post_dispatch(
		_info: &DispatchInfoOf<Call>,
		_post_info: &mut PostDispatchInfoOf<Call>,
		_len: usize,
		_result: &DispatchResult,
	) -> Result<(), TransactionValidityError> {
		Ok(())
	}
}

/// Helper macro to be used in a `impl TransactionExtension` block to add default implementations of
/// `weight`, `validate`, `prepare` or any combinations of the them.
///
/// The macro is to be used with 2 parameters, separated by ";":
/// - the `Call` type;
/// - the functions for which a default implementation should be generated, separated by " ";
///   available options are `weight`, `validate` and `prepare`.
///
/// Example usage:
/// ```nocompile
/// impl TransactionExtension<FirstCall> for EmptyExtension {
/// 	type Val = ();
/// 	type Pre = ();
///
/// 	impl_tx_ext_default!(FirstCall; weight validate prepare);
/// }
///
/// impl TransactionExtension<SecondCall> for SimpleExtension {
/// 	type Val = u32;
/// 	type Pre = ();
///
/// 	fn weight(&self, _: &SecondCall) -> Weight {
/// 		Weight::zero()
/// 	}
///
/// 	fn validate(
/// 			&self,
/// 			_origin: <T as Config>::RuntimeOrigin,
/// 			_call: &SecondCall,
/// 			_info: &DispatchInfoOf<SecondCall>,
/// 			_len: usize,
/// 			_self_implicit: Self::Implicit,
/// 			_inherited_implication: &impl Encode,
/// 		) -> ValidateResult<Self::Val, SecondCall> {
/// 		Ok((Default::default(), 42u32, origin))
/// 	}
///
/// 	impl_tx_ext_default!(SecondCall; prepare);
/// }
/// ```
#[macro_export]
macro_rules! impl_tx_ext_default {
	($call:ty ; , $( $rest:tt )*) => {
		impl_tx_ext_default!{$call ; $( $rest )*}
	};
	($call:ty ; validate $( $rest:tt )*) => {
		fn validate(
			&self,
			origin: $crate::traits::DispatchOriginOf<$call>,
			_call: &$call,
			_info: &$crate::traits::DispatchInfoOf<$call>,
			_len: usize,
			_self_implicit: Self::Implicit,
			_inherited_implication: &impl $crate::codec::Encode,
		) -> $crate::traits::ValidateResult<Self::Val, $call> {
			Ok((Default::default(), Default::default(), origin))
		}
		impl_tx_ext_default!{$call ; $( $rest )*}
	};
	($call:ty ; prepare $( $rest:tt )*) => {
		fn prepare(
			self,
			_val: Self::Val,
			_origin: &$crate::traits::DispatchOriginOf<$call>,
			_call: &$call,
			_info: &$crate::traits::DispatchInfoOf<$call>,
			_len: usize,
		) -> Result<Self::Pre, $crate::transaction_validity::TransactionValidityError> {
			Ok(Default::default())
		}
		impl_tx_ext_default!{$call ; $( $rest )*}
	};
	($call:ty ; weight $( $rest:tt )*) => {
		fn weight(&self, _call: &$call) -> $crate::Weight {
			$crate::Weight::zero()
		}
		impl_tx_ext_default!{$call ; $( $rest )*}
	};
	($call:ty ;) => {};
}

/// Information about a [`TransactionExtension`] for the runtime metadata.
pub struct TransactionExtensionMetadata {
	/// The unique identifier of the [`TransactionExtension`].
	pub identifier: &'static str,
	/// The type of the [`TransactionExtension`].
	pub ty: MetaType,
	/// The type of the [`TransactionExtension`] additional signed data for the payload.
	pub implicit: MetaType,
}

#[impl_for_tuples(1, 12)]
impl<Call: Dispatchable> TransactionExtension<Call> for Tuple {
	const IDENTIFIER: &'static str = "Use `metadata()`!";
	for_tuples!( type Implicit = ( #( Tuple::Implicit ),* ); );
	fn implicit(&self) -> Result<Self::Implicit, TransactionValidityError> {
		Ok(for_tuples!( ( #( Tuple.implicit()? ),* ) ))
	}
	fn metadata() -> Vec<TransactionExtensionMetadata> {
		let mut ids = Vec::new();
		for_tuples!( #( ids.extend(Tuple::metadata()); )* );
		ids
	}

	for_tuples!( type Val = ( #( Tuple::Val ),* ); );
	for_tuples!( type Pre = ( #( Tuple::Pre ),* ); );

	fn weight(&self, call: &Call) -> Weight {
		let mut weight = Weight::zero();
		for_tuples!( #( weight = weight.saturating_add(Tuple.weight(call)); )* );
		weight
	}

	fn validate(
		&self,
		origin: <Call as Dispatchable>::RuntimeOrigin,
		call: &Call,
		info: &DispatchInfoOf<Call>,
		len: usize,
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
				Tuple.validate(origin, call, info, len, item_implicit, &implications)?
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
	) -> Result<Self::Pre, TransactionValidityError> {
		Ok(for_tuples!( ( #(
			Tuple::prepare(self.Tuple, val.Tuple, origin, call, info, len)?
		),* ) ))
	}

	fn post_dispatch_details(
		pre: Self::Pre,
		info: &DispatchInfoOf<Call>,
		post_info: &PostDispatchInfoOf<Call>,
		len: usize,
		result: &DispatchResult,
	) -> Result<Weight, TransactionValidityError> {
		let mut total_unspent_weight = Weight::zero();
		for_tuples!( #({
			let unspent_weight = Tuple::post_dispatch_details(pre.Tuple, info, post_info, len, result)?;
			total_unspent_weight = total_unspent_weight.saturating_add(unspent_weight);
		})* );
		Ok(total_unspent_weight)
	}

	fn post_dispatch(
		pre: Self::Pre,
		info: &DispatchInfoOf<Call>,
		post_info: &mut PostDispatchInfoOf<Call>,
		len: usize,
		result: &DispatchResult,
	) -> Result<(), TransactionValidityError> {
		for_tuples!( #( Tuple::post_dispatch(pre.Tuple, info, post_info, len, result)?; )* );
		Ok(())
	}

	fn bare_validate(call: &Call, info: &DispatchInfoOf<Call>, len: usize) -> TransactionValidity {
		let valid = ValidTransaction::default();
		for_tuples!(#(
			let item_valid = Tuple::bare_validate(call, info, len)?;
			let valid = valid.combine_with(item_valid);
		)* );
		Ok(valid)
	}

	fn bare_validate_and_prepare(
		call: &Call,
		info: &DispatchInfoOf<Call>,
		len: usize,
	) -> Result<(), TransactionValidityError> {
		for_tuples!( #( Tuple::bare_validate_and_prepare(call, info, len)?; )* );
		Ok(())
	}

	fn bare_post_dispatch(
		info: &DispatchInfoOf<Call>,
		post_info: &mut PostDispatchInfoOf<Call>,
		len: usize,
		result: &DispatchResult,
	) -> Result<(), TransactionValidityError> {
		for_tuples!( #( Tuple::bare_post_dispatch(info, post_info, len, result)?; )* );
		Ok(())
	}
}

impl<Call: Dispatchable> TransactionExtension<Call> for () {
	const IDENTIFIER: &'static str = "UnitTransactionExtension";
	type Implicit = ();
	fn implicit(&self) -> sp_std::result::Result<Self::Implicit, TransactionValidityError> {
		Ok(())
	}
	type Val = ();
	type Pre = ();
	fn weight(&self, _call: &Call) -> Weight {
		Weight::zero()
	}
	fn validate(
		&self,
		origin: <Call as Dispatchable>::RuntimeOrigin,
		_call: &Call,
		_info: &DispatchInfoOf<Call>,
		_len: usize,
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
	) -> Result<(), TransactionValidityError> {
		Ok(())
	}
}
