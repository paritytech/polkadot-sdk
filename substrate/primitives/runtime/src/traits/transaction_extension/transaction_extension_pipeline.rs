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

//! The transaction extension pipeline struct.

use crate::{
	scale_info::TypeInfo,
	traits::{
		transaction_extension::{
			TransactionExtension, TransactionExtensionMetadata, ValidateResult,
		},
		DispatchInfoOf, DispatchOriginOf, Dispatchable, PostDispatchInfoOf,
	},
	transaction_validity::{
		TransactionSource, TransactionValidity, TransactionValidityError, ValidTransaction,
	},
	DispatchResult,
};
use alloc::vec::Vec;
use codec::{Decode, Encode};
use core::fmt::Debug;
use sp_weights::Weight;
use tuplex::PushBack;

macro_rules! declare_pipeline {
	($( $num:tt: $generic:ident, { $( $basket_0:tt )* }, { $( $basket_1:tt )* }, { $( $basket_2:tt )* }, )*) => {
		/// A pipeline of transaction extensions. Same as a tuple of transaction extensions, but
		/// support up to 32 elements.
		#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
		pub struct TransactionExtensionPipeline<
			$( $generic = (), )*
		>(
			$( pub $generic, )*
		);

		paste::paste! {
			$(
				impl< $( [< E $basket_0 >], )* >
				From<( $( [< E $basket_0 >], )* )>
				for TransactionExtensionPipeline< $( [< E $basket_0 >], )* >
				{
					fn from(e: ($( [< E $basket_0 >], )*)) -> Self {
						TransactionExtensionPipeline(
							$( e.$basket_0, )*
							$( { $basket_1; () }, )*
							$( { $basket_2; () }, )*
						)
					}
				}
			)*
		}

		/// Implicit type for `TransactionExtensionPipeline`.
		#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
		pub struct TransactionExtensionPipelineImplicit<
			$( $generic = (), )*
		>(
			$( pub $generic, )*
		);

		paste::paste! {
			$(
				impl< $( [< E $basket_0 >], )* >
				From<( $( [< E $basket_0 >], )* )>
				for TransactionExtensionPipelineImplicit< $( [< E $basket_0 >], )* >
				{
					fn from(e: ($( [< E $basket_0 >], )*)) -> Self {
						TransactionExtensionPipelineImplicit(
							$( e.$basket_0, )*
							$( { $basket_1; () }, )*
							$( { $basket_2; () }, )*
						)
					}
				}
			)*
		}

		impl<
			Call: Dispatchable,
			$( $generic: TransactionExtension<Call>, )*
		> TransactionExtension<Call>
		for TransactionExtensionPipeline<
			$( $generic, )*
		>
		{
			const IDENTIFIER: &'static str = "TransactionExtensionPipeline<Use `metadata()`!>";
			type Implicit = TransactionExtensionPipelineImplicit<
				$( <$generic as TransactionExtension<Call>>::Implicit, )*
			>;
			fn implicit(&self) -> Result<Self::Implicit, TransactionValidityError> {
				Ok(TransactionExtensionPipelineImplicit(
					$( self.$num.implicit()?, )*
				))
			}
			fn metadata() -> Vec<TransactionExtensionMetadata> {
				let mut ids = Vec::new();
				$( ids.extend($generic::metadata()); )*
				ids
			}
			type Val = ( $( <$generic as TransactionExtension<Call>>::Val, )* );
			type Pre = ( $( <$generic as TransactionExtension<Call>>::Pre, )* );
			fn weight(&self, call: &Call) -> Weight {
				Weight::zero()
					$( .saturating_add(self.$num.weight(call)) )*
			}
			fn validate(
				&self,
				origin: DispatchOriginOf<Call>,
				call: &Call,
				info: &DispatchInfoOf<Call>,
				len: usize,
				self_implicit: Self::Implicit,
				inherited_implication: &impl Encode,
				source: TransactionSource,
			) -> ValidateResult<Self::Val, Call> {
				let valid = ValidTransaction::default();
				let val = ();
				let explicit_implications = (
					$( &self.$num, )*
				);
				let implicit_implications = self_implicit;

				$(
					// Implication of this pipeline element not relevant for later items, so we pop it.
					let item = explicit_implications.$num;
					let item_implicit = implicit_implications.$num;
					let (item_valid, item_val, origin) = {
						let implications = (
							// The first is the implications born of the fact we return the mutated
							// origin.
							inherited_implication,
							// This is the explicitly made implication born of the fact the new origin is
							// passed into the next items in this pipeline-tuple.
							(
								( $( explicit_implications.$basket_1, )* ),
								( $( explicit_implications.$basket_2, )* ),
							),
							// This is the implicitly made implication born of the fact the new origin is
							// passed into the next items in this pipeline-tuple.
							(
								( $( &implicit_implications.$basket_1, )* ),
								( $( &implicit_implications.$basket_2, )* ),
							),
						);
						$generic::validate(item, origin, call, info, len, item_implicit, &implications, source)?
					};
					let valid = valid.combine_with(item_valid);
					let val = val.push_back(item_val);
				)*

				Ok((valid, val, origin))
			}
			fn prepare(
				self,
				val: Self::Val,
				origin: &DispatchOriginOf<Call>,
				call: &Call,
				info: &DispatchInfoOf<Call>,
				len: usize,
			) -> Result<Self::Pre, TransactionValidityError> {
				Ok((
					$( self.$num.prepare(val.$num, origin, call, info, len)?, )*
				))
			}
			fn post_dispatch_details(
				pre: Self::Pre,
				info: &DispatchInfoOf<Call>,
				post_info: &PostDispatchInfoOf<Call>,
				len: usize,
				result: &DispatchResult,
			) -> Result<Weight, TransactionValidityError> {
				let mut total_unspent_weight = Weight::zero();

				$(
					let unspent_weight = $generic::post_dispatch_details(pre.$num, info, post_info, len, result)?;
					total_unspent_weight = total_unspent_weight.saturating_add(unspent_weight);
				)*

				Ok(total_unspent_weight)

			}
			fn post_dispatch(
				pre: Self::Pre,
				info: &DispatchInfoOf<Call>,
				post_info: &mut PostDispatchInfoOf<Call>,
				len: usize,
				result: &DispatchResult,
			) -> Result<(), TransactionValidityError> {
				$(
					$generic::post_dispatch(pre.$num, info, post_info, len, result)?;
				)*
				Ok(())
			}
			fn bare_validate(call: &Call, info: &DispatchInfoOf<Call>, len: usize) -> TransactionValidity {
				let valid = ValidTransaction::default();
				$(
					let item_valid = $generic::bare_validate(call, info, len)?;
					let valid = valid.combine_with(item_valid);
				)*
				Ok(valid)
			}

			fn bare_validate_and_prepare(
				call: &Call,
				info: &DispatchInfoOf<Call>,
				len: usize,
			) -> Result<(), TransactionValidityError> {
				$( $generic::bare_validate_and_prepare(call, info, len)?; )*
				Ok(())
			}

			fn bare_post_dispatch(
				info: &DispatchInfoOf<Call>,
				post_info: &mut PostDispatchInfoOf<Call>,
				len: usize,
				result: &DispatchResult,
			) -> Result<(), TransactionValidityError> {
				$( $generic::bare_post_dispatch(info, post_info, len, result)?; )*
				Ok(())
			}
		}
	};
}

declare_pipeline!(
	0: TransactionExtension0,   { 0 }, { 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 }, { 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 },
	1: TransactionExtension1,   { 0 1 }, { 2 3 4 5 6 7 8 9 10 11 12 13 14 15 }, { 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 },
	2: TransactionExtension2,   { 0 1 2 }, { 3 4 5 6 7 8 9 10 11 12 13 14 15 }, { 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 },
	3: TransactionExtension3,   { 0 1 2 3 }, { 4 5 6 7 8 9 10 11 12 13 14 15 }, { 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 },
	4: TransactionExtension4,   { 0 1 2 3 4 }, { 5 6 7 8 9 10 11 12 13 14 15 }, { 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 },
	5: TransactionExtension5,   { 0 1 2 3 4 5 }, { 6 7 8 9 10 11 12 13 14 15 }, { 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 },
	6: TransactionExtension6,   { 0 1 2 3 4 5 6 }, { 7 8 9 10 11 12 13 14 15 }, { 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 },
	7: TransactionExtension7,   { 0 1 2 3 4 5 6 7 }, { 8 9 10 11 12 13 14 15 }, { 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 },
	8: TransactionExtension8,   { 0 1 2 3 4 5 6 7 8 }, { 9 10 11 12 13 14 15 }, { 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 },
	9: TransactionExtension9,   { 0 1 2 3 4 5 6 7 8 9 }, { 10 11 12 13 14 15 }, { 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 },
	10: TransactionExtension10, { 0 1 2 3 4 5 6 7 8 9 10 }, { 11 12 13 14 15 }, { 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 },
	11: TransactionExtension11, { 0 1 2 3 4 5 6 7 8 9 10 11 }, { 12 13 14 15 }, { 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 },
	12: TransactionExtension12, { 0 1 2 3 4 5 6 7 8 9 10 11 12 }, { 13 14 15 }, { 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 },
	13: TransactionExtension13, { 0 1 2 3 4 5 6 7 8 9 10 11 12 13 }, { 14 15 }, { 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 },
	14: TransactionExtension14, { 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 }, { 15 }, { 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 },
	15: TransactionExtension15, { 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 }, { }, { 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 },
	16: TransactionExtension16, { 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 }, { }, { 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 },
	17: TransactionExtension17, { 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 }, { }, { 18 19 20 21 22 23 24 25 26 27 28 29 30 31 },
	18: TransactionExtension18, { 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 }, { }, { 19 20 21 22 23 24 25 26 27 28 29 30 31 },
	19: TransactionExtension19, { 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 }, { }, { 20 21 22 23 24 25 26 27 28 29 30 31 },
	20: TransactionExtension20, { 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 }, { }, { 21 22 23 24 25 26 27 28 29 30 31 },
	21: TransactionExtension21, { 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 }, { }, { 22 23 24 25 26 27 28 29 30 31 },
	22: TransactionExtension22, { 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 }, { }, { 23 24 25 26 27 28 29 30 31 },
	23: TransactionExtension23, { 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 }, { }, { 24 25 26 27 28 29 30 31 },
	24: TransactionExtension24, { 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 }, { }, { 25 26 27 28 29 30 31 },
	25: TransactionExtension25, { 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 }, { }, { 26 27 28 29 30 31 },
	26: TransactionExtension26, { 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 }, { }, { 27 28 29 30 31 },
	27: TransactionExtension27, { 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 }, { }, { 28 29 30 31 },
	28: TransactionExtension28, { 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 }, { }, { 29 30 31 },
	29: TransactionExtension29, { 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 }, { }, { 30 31 },
	30: TransactionExtension30, { 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 }, { }, { 31 },
	31: TransactionExtension31, { 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 }, { }, { },
);

#[cfg(test)]
mod tests {
	use super::*;
	use std::cell::RefCell;

	const A_WEIGHT: Weight = Weight::from_all(3);
	const A_POST_DISPATCH_WEIGHT: Weight = Weight::from_all(1);
	const A_VAL: u32 = 4;
	const A_PRE: u32 = 5;
	const A_IMPLICIT: u64 = 6;
	const A_EXPLICIT: u64 = 7;
	#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
	struct TransactionExtensionA(u64);
	impl TransactionExtension<()> for TransactionExtensionA {
		const IDENTIFIER: &'static str = "TransactionExtensionA";
		type Implicit = u64;
		fn implicit(&self) -> Result<Self::Implicit, TransactionValidityError> {
			Ok(A_IMPLICIT)
		}
		type Val = u32;
		type Pre = u32;
		fn weight(&self, _call: &()) -> Weight {
			A_WEIGHT
		}
		fn validate(
			&self,
			origin: DispatchOriginOf<()>,
			_call: &(),
			_info: &DispatchInfoOf<()>,
			_len: usize,
			self_implicit: Self::Implicit,
			_inherited_implication: &impl Encode,
			_source: TransactionSource,
		) -> ValidateResult<Self::Val, ()> {
			assert_eq!(self_implicit, A_IMPLICIT);
			assert_eq!(self.0, A_EXPLICIT);
			Ok((ValidTransaction::default(), A_VAL, origin))
		}
		fn prepare(
			self,
			val: Self::Val,
			_origin: &DispatchOriginOf<()>,
			_call: &(),
			_info: &DispatchInfoOf<()>,
			_len: usize,
		) -> Result<Self::Pre, TransactionValidityError> {
			assert_eq!(val, A_VAL);
			Ok(A_PRE)
		}
		fn post_dispatch_details(
			_pre: Self::Pre,
			_info: &DispatchInfoOf<()>,
			_post_info: &PostDispatchInfoOf<()>,
			_len: usize,
			_result: &DispatchResult,
		) -> Result<Weight, TransactionValidityError> {
			Ok(A_POST_DISPATCH_WEIGHT)
		}
	}

	const B_WEIGHT: Weight = Weight::from_all(5);
	const B_POST_DISPATCH_WEIGHT: Weight = Weight::from_all(2);
	const B_VAL: u32 = 6;
	const B_PRE: u32 = 7;
	const B_IMPLICIT: u32 = 8;
	const B_EXPLICIT: u32 = 9;
	#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
	struct TransactionExtensionB(u32);
	impl TransactionExtension<()> for TransactionExtensionB {
		const IDENTIFIER: &'static str = "TransactionExtensionB";
		type Implicit = u32;
		fn implicit(&self) -> Result<Self::Implicit, TransactionValidityError> {
			Ok(B_IMPLICIT)
		}
		type Val = u32;
		type Pre = u32;
		fn weight(&self, _call: &()) -> Weight {
			B_WEIGHT
		}
		fn validate(
			&self,
			origin: DispatchOriginOf<()>,
			_call: &(),
			_info: &DispatchInfoOf<()>,
			_len: usize,
			self_implicit: Self::Implicit,
			_inherited_implication: &impl Encode,
			_source: TransactionSource,
		) -> ValidateResult<Self::Val, ()> {
			assert_eq!(self_implicit, B_IMPLICIT);
			assert_eq!(self.0, B_EXPLICIT);
			Ok((ValidTransaction::default(), B_VAL, origin))
		}
		fn prepare(
			self,
			val: Self::Val,
			_origin: &DispatchOriginOf<()>,
			_call: &(),
			_info: &DispatchInfoOf<()>,
			_len: usize,
		) -> Result<Self::Pre, TransactionValidityError> {
			assert_eq!(val, B_VAL);
			Ok(B_PRE)
		}
		fn post_dispatch_details(
			_pre: Self::Pre,
			_info: &DispatchInfoOf<()>,
			_post_info: &PostDispatchInfoOf<()>,
			_len: usize,
			_result: &DispatchResult,
		) -> Result<Weight, TransactionValidityError> {
			Ok(B_POST_DISPATCH_WEIGHT)
		}
	}

	thread_local! {
		pub static INHERITED_IMPLICATION: RefCell<Vec<u8>> = RefCell::new(vec![]);
	}

	#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
	struct TransactionExtensionCheck;
	impl TransactionExtension<()> for TransactionExtensionCheck {
		const IDENTIFIER: &'static str = "TransactionExtensionCheck";
		type Implicit = ();
		fn implicit(&self) -> Result<Self::Implicit, TransactionValidityError> {
			Ok(())
		}
		type Val = u32;
		type Pre = u32;
		fn weight(&self, _call: &()) -> Weight {
			Weight::zero()
		}
		fn validate(
			&self,
			origin: DispatchOriginOf<()>,
			_call: &(),
			_info: &DispatchInfoOf<()>,
			_len: usize,
			_self_implicit: Self::Implicit,
			inherited_implication: &impl Encode,
			_source: TransactionSource,
		) -> ValidateResult<Self::Val, ()> {
			INHERITED_IMPLICATION.with_borrow(|i| assert_eq!(*i, inherited_implication.encode()));
			Ok((ValidTransaction::default(), 0, origin))
		}
		fn prepare(
			self,
			_val: Self::Val,
			_origin: &DispatchOriginOf<()>,
			_call: &(),
			_info: &DispatchInfoOf<()>,
			_len: usize,
		) -> Result<Self::Pre, TransactionValidityError> {
			Ok(0)
		}
		fn post_dispatch_details(
			_pre: Self::Pre,
			_info: &DispatchInfoOf<()>,
			_post_info: &PostDispatchInfoOf<()>,
			_len: usize,
			_result: &DispatchResult,
		) -> Result<Weight, TransactionValidityError> {
			Ok(Weight::zero())
		}
	}

	#[test]
	fn inherited_implications_at_the_end() {
		let t1 = TransactionExtensionPipeline::from((
			TransactionExtensionA(A_EXPLICIT),
			TransactionExtensionB(B_EXPLICIT),
			TransactionExtensionCheck,
		));

		t1.validate(
			(),
			&(),
			&(),
			0,
			TransactionExtensionPipelineImplicit::from((A_IMPLICIT, B_IMPLICIT, ())),
			&(),
			TransactionSource::Local,
		)
		.unwrap();
	}

	#[test]
	fn inherited_implications_in_the_middle_1() {
		INHERITED_IMPLICATION.with_borrow_mut(|i| {
			*i = (B_EXPLICIT, B_IMPLICIT).encode();
		});

		let t1 = TransactionExtensionPipeline::from((
			TransactionExtensionA(A_EXPLICIT),
			TransactionExtensionCheck,
			TransactionExtensionB(B_EXPLICIT),
		));

		t1.validate(
			(),
			&(),
			&(),
			0,
			TransactionExtensionPipelineImplicit::from((A_IMPLICIT, (), B_IMPLICIT)),
			&(),
			TransactionSource::Local,
		)
		.unwrap();
	}

	#[test]
	fn inherited_implications_in_the_middle_2() {
		INHERITED_IMPLICATION.with_borrow_mut(|i| {
			*i = (B_EXPLICIT, A_EXPLICIT, B_IMPLICIT, A_IMPLICIT).encode();
		});

		let t2 = TransactionExtensionPipeline::from((
			TransactionExtensionA(A_EXPLICIT),
			TransactionExtensionCheck,
			TransactionExtensionB(B_EXPLICIT),
			TransactionExtensionA(A_EXPLICIT),
		));

		t2.validate(
			(),
			&(),
			&(),
			0,
			TransactionExtensionPipelineImplicit::from((A_IMPLICIT, (), B_IMPLICIT, A_IMPLICIT)),
			&(),
			TransactionSource::Local,
		)
		.unwrap();
	}

	#[test]
	fn inherited_implications_in_the_middle_3() {
		INHERITED_IMPLICATION.with_borrow_mut(|i| {
			*i = (
				(B_EXPLICIT, A_EXPLICIT, B_EXPLICIT, A_EXPLICIT),
				(B_EXPLICIT, A_EXPLICIT, B_EXPLICIT, A_EXPLICIT),
				(B_EXPLICIT, A_EXPLICIT, B_EXPLICIT, A_EXPLICIT),
				(B_EXPLICIT, A_EXPLICIT, B_EXPLICIT, A_EXPLICIT),
				(B_EXPLICIT, B_EXPLICIT, B_EXPLICIT, B_EXPLICIT),
				(B_IMPLICIT, A_IMPLICIT, B_IMPLICIT, A_IMPLICIT),
				(B_IMPLICIT, A_IMPLICIT, B_IMPLICIT, A_IMPLICIT),
				(B_IMPLICIT, A_IMPLICIT, B_IMPLICIT, A_IMPLICIT),
				(B_IMPLICIT, A_IMPLICIT, B_IMPLICIT, A_IMPLICIT),
				(B_IMPLICIT, B_IMPLICIT, B_IMPLICIT, B_IMPLICIT),
			)
				.encode();
		});

		let t3 = TransactionExtensionPipeline::from((
			TransactionExtensionA(A_EXPLICIT),
			TransactionExtensionCheck,
			TransactionExtensionB(B_EXPLICIT),
			TransactionExtensionA(A_EXPLICIT),
			TransactionExtensionB(B_EXPLICIT),
			TransactionExtensionA(A_EXPLICIT),
			TransactionExtensionB(B_EXPLICIT),
			TransactionExtensionA(A_EXPLICIT),
			TransactionExtensionB(B_EXPLICIT),
			TransactionExtensionA(A_EXPLICIT),
			TransactionExtensionB(B_EXPLICIT),
			TransactionExtensionA(A_EXPLICIT),
			TransactionExtensionB(B_EXPLICIT),
			TransactionExtensionA(A_EXPLICIT),
			TransactionExtensionB(B_EXPLICIT),
			TransactionExtensionA(A_EXPLICIT),
			TransactionExtensionB(B_EXPLICIT),
			TransactionExtensionA(A_EXPLICIT),
			TransactionExtensionB(B_EXPLICIT),
			TransactionExtensionB(B_EXPLICIT),
			TransactionExtensionB(B_EXPLICIT),
			TransactionExtensionB(B_EXPLICIT),
		));

		t3.validate(
			(),
			&(),
			&(),
			0,
			TransactionExtensionPipelineImplicit::from((
				A_IMPLICIT,
				(),
				B_IMPLICIT,
				A_IMPLICIT,
				B_IMPLICIT,
				A_IMPLICIT,
				B_IMPLICIT,
				A_IMPLICIT,
				B_IMPLICIT,
				A_IMPLICIT,
				B_IMPLICIT,
				A_IMPLICIT,
				B_IMPLICIT,
				A_IMPLICIT,
				B_IMPLICIT,
				A_IMPLICIT,
				B_IMPLICIT,
				A_IMPLICIT,
				B_IMPLICIT,
				B_IMPLICIT,
				B_IMPLICIT,
				B_IMPLICIT,
			)),
			&(),
			TransactionSource::Local,
		)
		.unwrap();
	}
}
