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

//! Support code for the runtime.
//!
//! ## Note on Tuple Traits
//!
//! Many of the traits defined in [`traits`] have auto-implementations on tuples as well. Usually,
//! the tuple is a function of number of pallets in the runtime. By default, the traits are
//! implemented for tuples of up to 64 items.
//
// If you have more pallets in your runtime, or for any other reason need more, enabled `tuples-96`
// or the `tuples-128` complication flag. Note that these features *will increase* the compilation
// of this crate.

#![cfg_attr(not(feature = "std"), no_std)]

/// Export ourself as `frame_support` to make tests happy.
#[doc(hidden)]
extern crate self as frame_support;

/// Private exports that are being used by macros.
///
/// The exports are not stable and should not be relied on.
#[doc(hidden)]
pub mod __private {
	pub use codec;
	pub use frame_metadata as metadata;
	pub use log;
	pub use paste;
	pub use scale_info;
	pub use serde;
	pub use sp_core::{OpaqueMetadata, Void};
	pub use sp_core_hashing_proc_macro;
	pub use sp_inherents;
	#[cfg(feature = "std")]
	pub use sp_io::TestExternalities;
	pub use sp_io::{self, hashing, storage::root as storage_root};
	pub use sp_metadata_ir as metadata_ir;
	#[cfg(feature = "std")]
	pub use sp_runtime::{bounded_btree_map, bounded_vec};
	pub use sp_runtime::{
		traits::Dispatchable, DispatchError, RuntimeDebug, StateVersion, TransactionOutcome,
	};
	#[cfg(feature = "std")]
	pub use sp_state_machine::BasicExternalities;
	pub use sp_std;
	pub use sp_tracing;
	pub use tt_call::*;
}

#[macro_use]
pub mod dispatch;
pub mod crypto;
pub mod dispatch_context;
mod hash;
pub mod inherent;
pub mod instances;
pub mod migrations;
pub mod storage;
#[cfg(test)]
mod tests;
pub mod traits;
pub mod weights;
#[doc(hidden)]
pub mod unsigned {
	#[doc(hidden)]
	pub use crate::sp_runtime::traits::ValidateUnsigned;
	#[doc(hidden)]
	pub use crate::sp_runtime::transaction_validity::{
		TransactionSource, TransactionValidity, TransactionValidityError, UnknownTransaction,
	};
}

#[cfg(any(feature = "std", feature = "runtime-benchmarks", feature = "try-runtime", test))]
pub use self::storage::storage_noop_guard::StorageNoopGuard;
pub use self::{
	dispatch::{Callable, Parameter},
	hash::{
		Blake2_128, Blake2_128Concat, Blake2_256, Hashable, Identity, ReversibleStorageHasher,
		StorageHasher, Twox128, Twox256, Twox64Concat,
	},
	storage::{
		bounded_btree_map::BoundedBTreeMap,
		bounded_btree_set::BoundedBTreeSet,
		bounded_vec::{BoundedSlice, BoundedVec},
		migration,
		weak_bounded_vec::WeakBoundedVec,
		IterableStorageDoubleMap, IterableStorageMap, IterableStorageNMap, StorageDoubleMap,
		StorageMap, StorageNMap, StoragePrefixedMap, StorageValue,
	},
};
pub use sp_runtime::{
	self, print, traits::Printable, ConsensusEngineId, MAX_MODULE_ERROR_ENCODED_SIZE,
};

use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_runtime::TypeId;

/// A unified log target for support operations.
pub const LOG_TARGET: &str = "runtime::frame-support";

/// A type that cannot be instantiated.
#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
pub enum Never {}

/// A pallet identifier. These are per pallet and should be stored in a registry somewhere.
#[derive(Clone, Copy, Eq, PartialEq, Encode, Decode, TypeInfo)]
pub struct PalletId(pub [u8; 8]);

impl TypeId for PalletId {
	const TYPE_ID: [u8; 4] = *b"modl";
}

/// Generate a [`#[pallet::storage]`](pallet_macros::storage) alias outside of a pallet.
///
/// This storage alias works similarly to the [`#[pallet::storage]`](pallet_macros::storage)
/// attribute macro. It supports [`StorageValue`](storage::types::StorageValue),
/// [`StorageMap`](storage::types::StorageMap),
/// [`StorageDoubleMap`](storage::types::StorageDoubleMap) and
/// [`StorageNMap`](storage::types::StorageNMap). The main difference to the normal
/// [`#[pallet::storage]`](pallet_macros::storage) is the flexibility around declaring the
/// storage prefix to use. The storage prefix determines where to find the value in the
/// storage. [`#[pallet::storage]`](pallet_macros::storage) uses the name of the pallet as
/// declared in [`construct_runtime!`].
///
/// The flexibility around declaring the storage prefix makes this macro very useful for
/// writing migrations etc.
///
/// # Examples
///
/// There are different ways to declare the `prefix` to use. The `prefix` type can either be
/// declared explicetly by passing it to the macro as an attribute or by letting the macro
/// guess on what the `prefix` type is. The `prefix` is always passed as the first generic
/// argument to the type declaration. When using [`#[pallet::storage]`](pallet_macros::storage)
/// this first generic argument is always `_`. Besides declaring the `prefix`, the rest of the
/// type declaration works as with [`#[pallet::storage]`](pallet_macros::storage).
///
/// 1. Use the `verbatim` prefix type. This prefix type uses the given identifier as the
/// `prefix`:
#[doc = docify::embed!("src/tests/storage_alias.rs", verbatim_attribute)]
///
/// 2. Use the `pallet_name` prefix type. This prefix type uses the name of the pallet as
/// configured in    [`construct_runtime!`] as the `prefix`:
#[doc = docify::embed!("src/tests/storage_alias.rs", pallet_name_attribute)]
/// It requires that the given prefix type implements
/// [`PalletInfoAccess`](traits::PalletInfoAccess) (which is always the case for FRAME pallet
/// structs). In the example above, `Pallet<T>` is the prefix type.
///
/// 3. Use the `dynamic` prefix type. This prefix type calls [`Get::get()`](traits::Get::get)
///    to get the `prefix`:
#[doc = docify::embed!("src/tests/storage_alias.rs", dynamic_attribute)]
/// It requires that the given prefix type implements [`Get<'static str>`](traits::Get).
///
/// 4. Let the macro "guess" what kind of prefix type to use. This only supports verbatim or
///    pallet name. The macro uses the presence of generic arguments to the prefix type as an
///    indication that it should use the pallet name as the `prefix`:
#[doc = docify::embed!("src/tests/storage_alias.rs", storage_alias_guess)]
pub use frame_support_procedural::storage_alias;

pub use frame_support_procedural::derive_impl;

/// Create new implementations of the [`Get`](crate::traits::Get) trait.
///
/// The so-called parameter type can be created in four different ways:
///
/// - Using `const` to create a parameter type that provides a `const` getter. It is required that
///   the `value` is const.
///
/// - Declare the parameter type without `const` to have more freedom when creating the value.
///
/// - Using `storage` to create a storage parameter type. This type is special as it tries to load
///   the value from the storage under a fixed key. If the value could not be found in the storage,
///   the given default value will be returned. It is required that the value implements
///   [`Encode`](codec::Encode) and [`Decode`](codec::Decode). The key for looking up the value in
///   the storage is built using the following formula:
///
///   `twox_128(":" ++ NAME ++ ":")` where `NAME` is the name that is passed as type name.
///
/// - Using `static` to create a static parameter type. Its value is being provided by a static
///   variable with the equivalent name in `UPPER_SNAKE_CASE`. An additional `set` function is
///   provided in this case to alter the static variable. **This is intended for testing ONLY and is
///   ONLY available when `std` is enabled.**
///
/// # Examples
///
/// ```
/// # use frame_support::traits::Get;
/// # use frame_support::parameter_types;
/// // This function cannot be used in a const context.
/// fn non_const_expression() -> u64 { 99 }
///
/// const FIXED_VALUE: u64 = 10;
/// parameter_types! {
///    pub const Argument: u64 = 42 + FIXED_VALUE;
///    /// Visibility of the type is optional
///    OtherArgument: u64 = non_const_expression();
///    pub storage StorageArgument: u64 = 5;
///    pub static StaticArgument: u32 = 7;
/// }
///
/// trait Config {
///    type Parameter: Get<u64>;
///    type OtherParameter: Get<u64>;
///    type StorageParameter: Get<u64>;
///    type StaticParameter: Get<u32>;
/// }
///
/// struct Runtime;
/// impl Config for Runtime {
///    type Parameter = Argument;
///    type OtherParameter = OtherArgument;
///    type StorageParameter = StorageArgument;
///    type StaticParameter = StaticArgument;
/// }
///
/// // In testing, `StaticArgument` can be altered later: `StaticArgument::set(8)`.
/// ```
///
/// # Invalid example:
///
/// ```compile_fail
/// # use frame_support::traits::Get;
/// # use frame_support::parameter_types;
/// // This function cannot be used in a const context.
/// fn non_const_expression() -> u64 { 99 }
///
/// parameter_types! {
///    pub const Argument: u64 = non_const_expression();
/// }
/// ```
#[macro_export]
macro_rules! parameter_types {
	(
		$( #[ $attr:meta ] )*
		$vis:vis const $name:ident $(< $($ty_params:ident),* >)?: $type:ty = $value:expr;
		$( $rest:tt )*
	) => (
		$( #[ $attr ] )*
		$vis struct $name $(
			< $($ty_params),* >( $($crate::__private::sp_std::marker::PhantomData<$ty_params>),* )
		)?;
		$crate::parameter_types!(IMPL_CONST $name , $type , $value $( $(, $ty_params)* )?);
		$crate::parameter_types!( $( $rest )* );
	);
	(
		$( #[ $attr:meta ] )*
		$vis:vis $name:ident $(< $($ty_params:ident),* >)?: $type:ty = $value:expr;
		$( $rest:tt )*
	) => (
		$( #[ $attr ] )*
		$vis struct $name $(
			< $($ty_params),* >( $($crate::__private::sp_std::marker::PhantomData<$ty_params>),* )
		)?;
		$crate::parameter_types!(IMPL $name, $type, $value $( $(, $ty_params)* )?);
		$crate::parameter_types!( $( $rest )* );
	);
	(
		$( #[ $attr:meta ] )*
		$vis:vis storage $name:ident $(< $($ty_params:ident),* >)?: $type:ty = $value:expr;
		$( $rest:tt )*
	) => (
		$( #[ $attr ] )*
		$vis struct $name $(
			< $($ty_params),* >( $($crate::__private::sp_std::marker::PhantomData<$ty_params>),* )
		)?;
		$crate::parameter_types!(IMPL_STORAGE $name, $type, $value $( $(, $ty_params)* )?);
		$crate::parameter_types!( $( $rest )* );
	);
	() => ();
	(IMPL_CONST $name:ident, $type:ty, $value:expr $(, $ty_params:ident)*) => {
		impl< $($ty_params),* > $name< $($ty_params),* > {
			/// Returns the value of this parameter type.
			pub const fn get() -> $type {
				$value
			}
		}

		impl<_I: From<$type> $(, $ty_params)*> $crate::traits::Get<_I> for $name< $($ty_params),* > {
			fn get() -> _I {
				_I::from(Self::get())
			}
		}

		impl< $($ty_params),* > $crate::traits::TypedGet for $name< $($ty_params),* > {
			type Type = $type;
			fn get() -> $type {
				Self::get()
			}
		}
	};
	(IMPL $name:ident, $type:ty, $value:expr $(, $ty_params:ident)*) => {
		impl< $($ty_params),* > $name< $($ty_params),* > {
			/// Returns the value of this parameter type.
			pub fn get() -> $type {
				$value
			}
		}

		impl<_I: From<$type>, $(, $ty_params)*> $crate::traits::Get<_I> for $name< $($ty_params),* > {
			fn get() -> _I {
				_I::from(Self::get())
			}
		}

		impl< $($ty_params),* > $crate::traits::TypedGet for $name< $($ty_params),* > {
			type Type = $type;
			fn get() -> $type {
				Self::get()
			}
		}
	};
	(IMPL_STORAGE $name:ident, $type:ty, $value:expr $(, $ty_params:ident)*) => {
		#[allow(unused)]
		impl< $($ty_params),* > $name< $($ty_params),* > {
			/// Returns the key for this parameter type.
			pub fn key() -> [u8; 16] {
				$crate::__private::sp_core_hashing_proc_macro::twox_128!(b":", $name, b":")
			}

			/// Set the value of this parameter type in the storage.
			///
			/// This needs to be executed in an externalities provided environment.
			pub fn set(value: &$type) {
				$crate::storage::unhashed::put(&Self::key(), value);
			}

			/// Returns the value of this parameter type.
			///
			/// This needs to be executed in an externalities provided environment.
			#[allow(unused)]
			pub fn get() -> $type {
				$crate::storage::unhashed::get(&Self::key()).unwrap_or_else(|| $value)
			}
		}

		impl<_I: From<$type> $(, $ty_params)*> $crate::traits::Get<_I> for $name< $($ty_params),* > {
			fn get() -> _I {
				_I::from(Self::get())
			}
		}

		impl< $($ty_params),* > $crate::traits::TypedGet for $name< $($ty_params),* > {
			type Type = $type;
			fn get() -> $type {
				Self::get()
			}
		}
	};
	(
		$( #[ $attr:meta ] )*
		$vis:vis static $name:ident: $type:ty = $value:expr;
		$( $rest:tt )*
	) => (
		$crate::parameter_types_impl_thread_local!(
			$( #[ $attr ] )*
			$vis static $name: $type = $value;
		);
		$crate::parameter_types!( $( $rest )* );
	);
}

#[cfg(not(feature = "std"))]
#[macro_export]
macro_rules! parameter_types_impl_thread_local {
	( $( $any:tt )* ) => {
		compile_error!("static parameter types is only available in std and for testing.");
	};
}

#[cfg(feature = "std")]
#[macro_export]
macro_rules! parameter_types_impl_thread_local {
	(
		$(
			$( #[ $attr:meta ] )*
			$vis:vis static $name:ident: $type:ty = $value:expr;
		)*
	) => {
		$crate::parameter_types_impl_thread_local!(
			IMPL_THREAD_LOCAL $( $vis, $name, $type, $value, )*
		);
		$crate::__private::paste::item! {
			$crate::parameter_types!(
				$(
					$( #[ $attr ] )*
					$vis $name: $type = [<$name:snake:upper>].with(|v| v.borrow().clone());
				)*
			);
			$(
				impl $name {
					/// Set the internal value.
					pub fn set(t: $type) {
						[<$name:snake:upper>].with(|v| *v.borrow_mut() = t);
					}

					/// Mutate the internal value in place.
					#[allow(unused)]
					pub fn mutate<R, F: FnOnce(&mut $type) -> R>(mutate: F) -> R{
						let mut current = Self::get();
						let result = mutate(&mut current);
						Self::set(current);
						result
					}

					/// Get current value and replace with initial value of the parameter type.
					#[allow(unused)]
					pub fn take() -> $type {
						let current = Self::get();
						Self::set($value);
						current
					}
				}
			)*
		}
	};
	(IMPL_THREAD_LOCAL $( $vis:vis, $name:ident, $type:ty, $value:expr, )* ) => {
		$crate::__private::paste::item! {
			thread_local! {
				$(
					pub static [<$name:snake:upper>]: std::cell::RefCell<$type> =
						std::cell::RefCell::new($value);
				)*
			}
		}
	};
}

/// Macro for easily creating a new implementation of both the `Get` and `Contains` traits. Use
/// exactly as with `parameter_types`, only the type must be `Ord`.
#[macro_export]
macro_rules! ord_parameter_types {
	(
		$( #[ $attr:meta ] )*
		$vis:vis const $name:ident: $type:ty = $value:expr;
		$( $rest:tt )*
	) => (
		$( #[ $attr ] )*
		$vis struct $name;
		$crate::parameter_types!{IMPL $name , $type , $value}
		$crate::ord_parameter_types!{IMPL $name , $type , $value}
		$crate::ord_parameter_types!{ $( $rest )* }
	);
	() => ();
	(IMPL $name:ident , $type:ty , $value:expr) => {
		impl $crate::traits::SortedMembers<$type> for $name {
			fn contains(t: &$type) -> bool { &$value == t }
			fn sorted_members() -> $crate::__private::sp_std::prelude::Vec<$type> { vec![$value] }
			fn count() -> usize { 1 }
			#[cfg(feature = "runtime-benchmarks")]
			fn add(_: &$type) {}
		}
		impl $crate::traits::Contains<$type> for $name {
			fn contains(t: &$type) -> bool { &$value == t }
		}
	}
}

/// Print out a formatted message.
///
/// # Example
///
/// ```
/// frame_support::runtime_print!("my value is {}", 3);
/// ```
#[macro_export]
macro_rules! runtime_print {
	($($arg:tt)+) => {
		{
			use core::fmt::Write;
			let mut w = $crate::__private::sp_std::Writer::default();
			let _ = core::write!(&mut w, $($arg)+);
			$crate::__private::sp_io::misc::print_utf8(&w.inner())
		}
	}
}

/// Print out the debuggable type.
pub fn debug(data: &impl sp_std::fmt::Debug) {
	runtime_print!("{:?}", data);
}

#[doc(inline)]
pub use frame_support_procedural::{
	construct_runtime, match_and_insert, transactional, PalletError, RuntimeDebugNoBound,
};

#[doc(hidden)]
pub use frame_support_procedural::{__create_tt_macro, __generate_dummy_part_checker};

/// Derive [`Clone`] but do not bound any generic.
///
/// This is useful for type generic over runtime:
/// ```
/// # use frame_support::CloneNoBound;
/// trait Config {
/// 		type C: Clone;
/// }
///
/// // Foo implements [`Clone`] because `C` bounds [`Clone`].
/// // Otherwise compilation will fail with an output telling `c` doesn't implement [`Clone`].
/// #[derive(CloneNoBound)]
/// struct Foo<T: Config> {
/// 		c: T::C,
/// }
/// ```
pub use frame_support_procedural::CloneNoBound;

/// Derive [`Eq`] but do not bound any generic.
///
/// This is useful for type generic over runtime:
/// ```
/// # use frame_support::{EqNoBound, PartialEqNoBound};
/// trait Config {
/// 		type C: Eq;
/// }
///
/// // Foo implements [`Eq`] because `C` bounds [`Eq`].
/// // Otherwise compilation will fail with an output telling `c` doesn't implement [`Eq`].
/// #[derive(PartialEqNoBound, EqNoBound)]
/// struct Foo<T: Config> {
/// 		c: T::C,
/// }
/// ```
pub use frame_support_procedural::EqNoBound;

/// Derive [`PartialEq`] but do not bound any generic.
///
/// This is useful for type generic over runtime:
/// ```
/// # use frame_support::PartialEqNoBound;
/// trait Config {
/// 		type C: PartialEq;
/// }
///
/// // Foo implements [`PartialEq`] because `C` bounds [`PartialEq`].
/// // Otherwise compilation will fail with an output telling `c` doesn't implement [`PartialEq`].
/// #[derive(PartialEqNoBound)]
/// struct Foo<T: Config> {
/// 		c: T::C,
/// }
/// ```
pub use frame_support_procedural::PartialEqNoBound;

/// Derive [`Debug`] but do not bound any generic.
///
/// This is useful for type generic over runtime:
/// ```
/// # use frame_support::DebugNoBound;
/// # use core::fmt::Debug;
/// trait Config {
/// 		type C: Debug;
/// }
///
/// // Foo implements [`Debug`] because `C` bounds [`Debug`].
/// // Otherwise compilation will fail with an output telling `c` doesn't implement [`Debug`].
/// #[derive(DebugNoBound)]
/// struct Foo<T: Config> {
/// 		c: T::C,
/// }
/// ```
pub use frame_support_procedural::DebugNoBound;

/// Derive [`Default`] but do not bound any generic.
///
/// This is useful for type generic over runtime:
/// ```
/// # use frame_support::DefaultNoBound;
/// # use core::default::Default;
/// trait Config {
/// 	type C: Default;
/// }
///
/// // Foo implements [`Default`] because `C` bounds [`Default`].
/// // Otherwise compilation will fail with an output telling `c` doesn't implement [`Default`].
/// #[derive(DefaultNoBound)]
/// struct Foo<T: Config> {
/// 	c: T::C,
/// }
///
/// // Also works with enums, by specifying the default with #[default]:
/// #[derive(DefaultNoBound)]
/// enum Bar<T: Config> {
/// 	// Bar will implement Default as long as all of the types within Baz also implement default.
/// 	#[default]
/// 	Baz(T::C),
/// 	Quxx,
/// }
/// ```
pub use frame_support_procedural::DefaultNoBound;

/// Assert the annotated function is executed within a storage transaction.
///
/// The assertion is enabled for native execution and when `debug_assertions` are enabled.
///
/// # Example
///
/// ```
/// # use frame_support::{
/// # 	require_transactional, transactional, dispatch::DispatchResult
/// # };
///
/// #[require_transactional]
/// fn update_all(value: u32) -> DispatchResult {
/// 	// Update multiple storages.
/// 	// Return `Err` to indicate should revert.
/// 	Ok(())
/// }
///
/// #[transactional]
/// fn safe_update(value: u32) -> DispatchResult {
/// 	// This is safe
/// 	update_all(value)
/// }
///
/// fn unsafe_update(value: u32) -> DispatchResult {
/// 	// this may panic if unsafe_update is not called within a storage transaction
/// 	update_all(value)
/// }
/// ```
pub use frame_support_procedural::require_transactional;

/// Convert the current crate version into a [`CrateVersion`](crate::traits::CrateVersion).
///
/// It uses the `CARGO_PKG_VERSION_MAJOR`, `CARGO_PKG_VERSION_MINOR` and
/// `CARGO_PKG_VERSION_PATCH` environment variables to fetch the crate version.
/// This means that the [`CrateVersion`](crate::traits::CrateVersion)
/// object will correspond to the version of the crate the macro is called in!
///
/// # Example
///
/// ```
/// # use frame_support::{traits::CrateVersion, crate_to_crate_version};
/// const Version: CrateVersion = crate_to_crate_version!();
/// ```
pub use frame_support_procedural::crate_to_crate_version;

/// Return Err of the expression: `return Err($expression);`.
///
/// Used as `fail!(expression)`.
#[macro_export]
macro_rules! fail {
	( $y:expr ) => {{
		return Err($y.into())
	}};
}

/// Evaluate `$x:expr` and if not true return `Err($y:expr)`.
///
/// Used as `ensure!(expression_to_ensure, expression_to_return_on_false)`.
#[macro_export]
macro_rules! ensure {
	( $x:expr, $y:expr $(,)? ) => {{
		if !$x {
			$crate::fail!($y);
		}
	}};
}

/// Evaluate an expression, assert it returns an expected `Err` value and that
/// runtime storage has not been mutated (i.e. expression is a no-operation).
///
/// Used as `assert_noop(expression_to_assert, expected_error_expression)`.
#[macro_export]
macro_rules! assert_noop {
	(
		$x:expr,
		$y:expr $(,)?
	) => {
		let h = $crate::__private::storage_root($crate::__private::StateVersion::V1);
		$crate::assert_err!($x, $y);
		assert_eq!(
			h,
			$crate::__private::storage_root($crate::__private::StateVersion::V1),
			"storage has been mutated"
		);
	};
}

/// Evaluate any expression and assert that runtime storage has not been mutated
/// (i.e. expression is a storage no-operation).
///
/// Used as `assert_storage_noop(expression_to_assert)`.
#[macro_export]
macro_rules! assert_storage_noop {
	(
		$x:expr
	) => {
		let h = $crate::__private::storage_root($crate::__private::StateVersion::V1);
		$x;
		assert_eq!(h, $crate::__private::storage_root($crate::__private::StateVersion::V1));
	};
}

/// Assert an expression returns an error specified.
///
/// Used as `assert_err!(expression_to_assert, expected_error_expression)`
#[macro_export]
macro_rules! assert_err {
	( $x:expr , $y:expr $(,)? ) => {
		assert_eq!($x, Err($y.into()));
	};
}

/// Assert an expression returns an error specified.
///
/// This can be used on `DispatchResultWithPostInfo` when the post info should
/// be ignored.
#[macro_export]
macro_rules! assert_err_ignore_postinfo {
	( $x:expr , $y:expr $(,)? ) => {
		$crate::assert_err!($x.map(|_| ()).map_err(|e| e.error), $y);
	};
}

/// Assert an expression returns error with the given weight.
#[macro_export]
macro_rules! assert_err_with_weight {
	($call:expr, $err:expr, $weight:expr $(,)? ) => {
		if let Err(dispatch_err_with_post) = $call {
			$crate::assert_err!($call.map(|_| ()).map_err(|e| e.error), $err);
			assert_eq!(dispatch_err_with_post.post_info.actual_weight, $weight);
		} else {
			panic!("expected Err(_), got Ok(_).")
		}
	};
}

/// Panic if an expression doesn't evaluate to `Ok`.
///
/// Used as `assert_ok!(expression_to_assert, expected_ok_expression)`,
/// or `assert_ok!(expression_to_assert)` which would assert against `Ok(())`.
#[macro_export]
macro_rules! assert_ok {
	( $x:expr $(,)? ) => {
		let is = $x;
		match is {
			Ok(_) => (),
			_ => assert!(false, "Expected Ok(_). Got {:#?}", is),
		}
	};
	( $x:expr, $y:expr $(,)? ) => {
		assert_eq!($x, Ok($y));
	};
}

/// Assert that the maximum encoding size does not exceed the value defined in
/// [`MAX_MODULE_ERROR_ENCODED_SIZE`] during compilation.
///
/// This macro is intended to be used in conjunction with `tt_call!`.
#[macro_export]
macro_rules! assert_error_encoded_size {
	{
		path = [{ $($path:ident)::+ }]
		runtime = [{ $runtime:ident }]
		assert_message = [{ $assert_message:literal }]
		error = [{ $error:ident }]
	} => {
		const _: () = assert!(
			<
				$($path::)+$error<$runtime> as $crate::traits::PalletError
			>::MAX_ENCODED_SIZE <= $crate::MAX_MODULE_ERROR_ENCODED_SIZE,
			$assert_message
		);
	};
	{
		path = [{ $($path:ident)::+ }]
		runtime = [{ $runtime:ident }]
		assert_message = [{ $assert_message:literal }]
	} => {};
}

/// Do something hypothetically by rolling back any changes afterwards.
///
/// Returns the original result of the closure.
#[macro_export]
#[cfg(feature = "experimental")]
macro_rules! hypothetically {
	( $e:expr ) => {
		$crate::storage::transactional::with_transaction(|| -> $crate::__private::TransactionOutcome<Result<_, $crate::__private::DispatchError>> {
			$crate::__private::TransactionOutcome::Rollback(Ok($e))
		},
		).expect("Always returning Ok; qed")
	};
}

/// Assert something to be *hypothetically* `Ok`, without actually committing it.
///
/// Reverts any storage changes made by the closure.
#[macro_export]
#[cfg(feature = "experimental")]
macro_rules! hypothetically_ok {
	($e:expr $(, $args:expr)* $(,)?) => {
		$crate::assert_ok!($crate::hypothetically!($e) $(, $args)*);
	};
}

#[doc(hidden)]
pub use serde::{Deserialize, Serialize};

#[doc(hidden)]
#[cfg(not(no_std))]
pub use macro_magic;

/// Prelude to be used for pallet testing, for ease of use.
#[cfg(feature = "std")]
pub mod testing_prelude {
	pub use super::{
		assert_err, assert_err_ignore_postinfo, assert_err_with_weight, assert_error_encoded_size,
		assert_noop, assert_ok, assert_storage_noop, parameter_types, traits::Get,
	};
	pub use sp_arithmetic::assert_eq_error_rate;
	pub use sp_runtime::{bounded_btree_map, bounded_vec};
}

/// Prelude to be used alongside pallet macro, for ease of use.
pub mod pallet_prelude {
	pub use crate::{
		defensive, defensive_assert,
		dispatch::{DispatchClass, DispatchResult, DispatchResultWithPostInfo, Parameter, Pays},
		ensure,
		inherent::{InherentData, InherentIdentifier, ProvideInherent},
		storage,
		storage::{
			bounded_btree_map::BoundedBTreeMap,
			bounded_btree_set::BoundedBTreeSet,
			bounded_vec::BoundedVec,
			types::{
				CountedStorageMap, CountedStorageNMap, Key as NMapKey, OptionQuery, ResultQuery,
				StorageDoubleMap, StorageMap, StorageNMap, StorageValue, ValueQuery,
			},
			weak_bounded_vec::WeakBoundedVec,
			StorageList,
		},
		traits::{
			BuildGenesisConfig, ConstU32, EnsureOrigin, Get, GetDefault, GetStorageVersion, Hooks,
			IsType, PalletInfoAccess, StorageInfoTrait, StorageVersion, Task, TypedGet,
		},
		Blake2_128, Blake2_128Concat, Blake2_256, CloneNoBound, DebugNoBound, EqNoBound, Identity,
		PartialEqNoBound, RuntimeDebugNoBound, Twox128, Twox256, Twox64Concat,
	};
	pub use codec::{Decode, Encode, MaxEncodedLen};
	pub use frame_support::pallet_macros::*;
	/// The optional attribute `#[inject_runtime_type]` can be attached to `RuntimeCall`,
	/// `RuntimeEvent`, `RuntimeOrigin` or `PalletInfo` in an impl statement that has
	/// `#[register_default_impl]` attached to indicate that this item is generated by
	/// `construct_runtime`.
	///
	/// Attaching this attribute to such an item ensures that the combined impl generated via
	/// [`#[derive_impl(..)]`](`macro@super::derive_impl`)  will use the correct type
	/// auto-generated by `construct_runtime!`.
	#[doc = docify::embed!("src/tests/inject_runtime_type.rs", derive_impl_works_with_runtime_type_injection)]
	///
	/// However, if `no_aggregated_types` is specified while using
	/// `[`#[derive_impl(..)]`](`macro@super::derive_impl`)`, then these items are attached
	/// verbatim to the combined impl.
	#[doc = docify::embed!("src/tests/inject_runtime_type.rs", derive_impl_works_with_no_aggregated_types)]
	pub use frame_support_procedural::inject_runtime_type;
	pub use frame_support_procedural::register_default_impl;
	pub use scale_info::TypeInfo;
	pub use sp_inherents::MakeFatalError;
	pub use sp_runtime::{
		traits::{MaybeSerializeDeserialize, Member, ValidateUnsigned},
		transaction_validity::{
			InvalidTransaction, TransactionLongevity, TransactionPriority, TransactionSource,
			TransactionTag, TransactionValidity, TransactionValidityError, UnknownTransaction,
			ValidTransaction,
		},
		DispatchError, RuntimeDebug, MAX_MODULE_ERROR_ENCODED_SIZE,
	};
	pub use sp_std::marker::PhantomData;
	pub use sp_weights::Weight;
}

/// The pallet struct placeholder `#[pallet::pallet]` is mandatory and allows you to
/// specify pallet information.
///
/// The struct must be defined as follows:
/// ```
/// #[frame_support::pallet]
/// mod pallet {
/// 	#[pallet::pallet]         // <- the macro
/// 	pub struct Pallet<T>(_);  // <- the struct definition
///
/// 	#[pallet::config]
/// 	pub trait Config: frame_system::Config {}
/// }
/// ```
/// I.e. a regular struct definition named `Pallet`, with generic T and no where clause.
///
/// ## Macro expansion:
///
/// The macro adds this attribute to the Pallet struct definition:
/// ```ignore
/// #[derive(
/// 	frame_support::CloneNoBound,
/// 	frame_support::EqNoBound,
/// 	frame_support::PartialEqNoBound,
/// 	frame_support::RuntimeDebugNoBound,
/// )]
/// ```
/// and replaces the type `_` with `PhantomData<T>`. It also implements on the pallet:
/// * [`GetStorageVersion`](frame_support::traits::GetStorageVersion)
/// * [`OnGenesis`](frame_support::traits::OnGenesis): contains some logic to write the pallet
///   version into storage.
///
/// It declares `type Module` type alias for `Pallet`, used by `construct_runtime`.
///
/// It implements [`PalletInfoAccess`](frame_support::traits::PalletInfoAccess) on `Pallet`
/// to ease access to pallet information given by [`frame_support::traits::PalletInfo`].
/// (The implementation uses the associated type [`frame_support::traits::PalletInfo`]).
///
/// It implements [`StorageInfoTrait`](frame_support::traits::StorageInfoTrait) on `Pallet`
/// which give information about all storages.
///
/// If the attribute [`generate_store`](frame_support::pallet_macros::generate_store) is
/// set then the macro creates the trait `Store` and implements it on `Pallet`.
///
/// If the attribute `set_storage_max_encoded_len` is set then the macro calls
/// [`StorageInfoTrait`](frame_support::traits::StorageInfoTrait) for each storage in the
/// implementation of [`StorageInfoTrait`](frame_support::traits::StorageInfoTrait) for the
/// pallet. Otherwise it implements
/// [`StorageInfoTrait`](frame_support::traits::StorageInfoTrait) for the pallet using the
/// [`PartialStorageInfoTrait`](frame_support::traits::PartialStorageInfoTrait)
/// implementation of storages.
///
/// ## Dev Mode (`#[pallet(dev_mode)]`)
///
/// Specifying the argument `dev_mode` will allow you to enable dev mode for a pallet. The
/// aim of dev mode is to loosen some of the restrictions and requirements placed on
/// production pallets for easy tinkering and development. Dev mode pallets should not be
/// used in production. Enabling dev mode has the following effects:
///
/// * Weights no longer need to be specified on every `#[pallet::call]` declaration. By
///   default, dev mode pallets will assume a weight of zero (`0`) if a weight is not
///   specified. This is equivalent to specifying `#[weight(0)]` on all calls that do not
///   specify a weight.
/// * Call indices no longer need to be specified on every `#[pallet::call]` declaration. By
///   default, dev mode pallets will assume a call index based on the order of the call.
/// * All storages are marked as unbounded, meaning you do not need to implement
///   [`MaxEncodedLen`](frame_support::pallet_prelude::MaxEncodedLen) on storage types. This is
///   equivalent to specifying `#[pallet::unbounded]` on all storage type definitions.
/// * Storage hashers no longer need to be specified and can be replaced by `_`. In dev mode,
///   these will be replaced by `Blake2_128Concat`. In case of explicit key-binding, `Hasher`
///   can simply be ignored when in `dev_mode`.
///
/// Note that the `dev_mode` argument can only be supplied to the `#[pallet]` or
/// `#[frame_support::pallet]` attribute macro that encloses your pallet module. This
/// argument cannot be specified anywhere else, including but not limited to the
/// `#[pallet::pallet]` attribute macro.
///
/// <div class="example-wrap" style="display:inline-block"><pre class="compile_fail"
/// style="white-space:normal;font:inherit;">
/// <strong>WARNING</strong>:
/// You should not deploy or use dev mode pallets in production. Doing so can break your
/// chain and therefore should never be done. Once you are done tinkering, you should
/// remove the 'dev_mode' argument from your #[pallet] declaration and fix any compile
/// errors before attempting to use your pallet in a production scenario.
/// </pre></div>
///
/// ## Runtime Metadata Documentation
///
/// The documentation added to this pallet is included in the runtime metadata.
///
/// The documentation can be defined in the following ways:
///
/// ```ignore
/// #[pallet::pallet]
/// /// Documentation for pallet 1
/// #[doc = "Documentation for pallet 2"]
/// #[doc = include_str!("../README.md")]
/// #[pallet_doc("../doc1.md")]
/// #[pallet_doc("../doc2.md")]
/// pub mod pallet {}
/// ```
///
/// The runtime metadata for this pallet contains the following
///  - "Documentation for pallet 1" (captured from `///`)
///  - "Documentation for pallet 2"  (captured from `#[doc]`)
///  - content of ../README.md       (captured from `#[doc]` with `include_str!`)
///  - content of "../doc1.md"       (captured from `pallet_doc`)
///  - content of "../doc2.md"       (captured from `pallet_doc`)
///
/// ### `doc` attribute
///
/// The value of the `doc` attribute is included in the runtime metadata, as well as
/// expanded on the pallet module. The previous example is expanded to:
///
/// ```ignore
/// /// Documentation for pallet 1
/// /// Documentation for pallet 2
/// /// Content of README.md
/// pub mod pallet {}
/// ```
///
/// If you want to specify the file from which the documentation is loaded, you can use the
/// `include_str` macro. However, if you only want the documentation to be included in the
/// runtime metadata, use the `pallet_doc` attribute.
///
/// ### `pallet_doc` attribute
///
/// Unlike the `doc` attribute, the documentation provided to the `pallet_doc` attribute is
/// not inserted on the module.
///
/// The `pallet_doc` attribute can only be provided with one argument,
/// which is the file path that holds the documentation to be added to the metadata.
///
/// This approach is beneficial when you use the `include_str` macro at the beginning of
/// the file and want that documentation to extend to the runtime metadata, without
/// reiterating the documentation on the pallet module itself.
pub use frame_support_procedural::pallet;

/// Contains macro stubs for all of the pallet:: macros
pub mod pallet_macros {
	pub use frame_support_procedural::{
		composite_enum, config, disable_frame_system_supertrait_check, extra_constants,
		generate_store, getter, import_section, inherent, no_default, no_default_bounds, origin,
		pallet_section, storage_prefix, unbounded, weight, whitelist_storage,
	};

	/// The `#[pallet::validate_unsigned]` attribute allows the pallet to validate some
	/// unsigned transaction:
	///
	/// Item must be defined as:
	///
	/// ```
	/// #[frame_support::pallet]
	/// mod pallet {
	/// 	use frame_support::pallet_prelude::*;
	///
	/// 	#[pallet::pallet]
	/// 	pub struct Pallet<T>(_);
	///
	/// 	#[pallet::validate_unsigned]
	/// 	impl<T: Config> sp_runtime::traits::ValidateUnsigned for Pallet<T> {
	/// 		type Call = Call<T>;
	///
	/// 		fn validate_unsigned(_source: TransactionSource, _call: &Self::Call) -> TransactionValidity {
	/// 			// Your implementation details here
	/// 			unimplemented!()
	/// 		}
	/// 	}
	///
	/// 	#[pallet::config]
	/// 	pub trait Config: frame_system::Config {}
	/// }
	/// ```
	///
	/// I.e. a trait implementation with bound `T: Config`, of trait
	/// [`ValidateUnsigned`](frame_support::pallet_prelude::ValidateUnsigned) for
	/// type `Pallet<T>`, and some optional where clause.
	///
	/// NOTE: There is also the `sp_runtime::traits::SignedExtension` trait that can be used to
	/// add some specific logic for transaction validation.
	///
	/// ## Macro expansion
	///
	/// The macro currently makes no use of this information, but it might use this information
	/// in the future to give information directly to [`frame_support::construct_runtime`].
	pub use frame_support_procedural::validate_unsigned;

	/// The `#[pallet::type_value]` attribute lets you define a struct implementing the
	/// [`Get`](frame_support::traits::Get) trait to ease the use of storage types. This
	/// attribute is meant to be used alongside [`#[pallet::storage]`](`macro@storage`) to
	/// define a storage's default value. This attribute can be used multiple times.
	///
	/// Item must be defined as:
	///
	/// ```
	/// #[frame_support::pallet]
	/// mod pallet {
	/// 	use sp_runtime::FixedU128;
	/// 	use frame_support::pallet_prelude::*;
	///
	/// 	#[pallet::pallet]
	/// 	pub struct Pallet<T>(_);
	///
	/// 	#[pallet::storage]
	/// 	pub(super) type SomeStorage<T: Config> =
	/// 		StorageValue<_, FixedU128, ValueQuery, DefaultForSomeValue>;
	///
	/// 	// Define default for ParachainId
	/// 	#[pallet::type_value]
	/// 	pub fn DefaultForSomeValue() -> FixedU128 {
	/// 		FixedU128::from_u32(1)
	/// 	}
	///
	/// 	#[pallet::config]
	/// 	pub trait Config: frame_system::Config {}
	/// }
	/// ```
	///
	/// ## Macro expansion
	///
	/// The macro renames the function to some internal name, generates a struct with the
	/// original name of the function and its generic, and implements `Get<$ReturnType>` by
	/// calling the user defined function.
	pub use frame_support_procedural::type_value;

	/// Because the `pallet::pallet` macro implements
	/// [`GetStorageVersion`](frame_support::traits::GetStorageVersion), the current storage
	/// version needs to be communicated to the macro. This can be done by using the
	/// `pallet::storage_version` attribute:
	///
	/// ```
	/// #[frame_support::pallet]
	/// mod pallet {
	/// 	use frame_support::pallet_prelude::StorageVersion;
	/// 	use frame_support::traits::GetStorageVersion;
	///
	/// 	const STORAGE_VERSION: StorageVersion = StorageVersion::new(5);
	///
	/// 	#[pallet::pallet]
	/// 	#[pallet::storage_version(STORAGE_VERSION)]
	/// 	pub struct Pallet<T>(_);
	///
	/// 	#[pallet::config]
	/// 	pub trait Config: frame_system::Config {}
	/// }
	/// ```
	///
	/// If not present, the current storage version is set to the default value.
	pub use frame_support_procedural::storage_version;

	/// The `#[pallet::hooks]` attribute allows you to specify a `Hooks` implementation for
	/// `Pallet` that specifies pallet-specific logic.
	///
	/// The item the attribute attaches to must be defined as follows:
	///
	/// ```
	/// #[frame_support::pallet]
	/// mod pallet {
	/// 	use frame_support::pallet_prelude::*;
	/// 	use frame_system::pallet_prelude::*;
	///
	/// 	#[pallet::pallet]
	/// 	pub struct Pallet<T>(_);
	///
	/// 	#[pallet::hooks]
	/// 	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
	/// 		// Implement hooks here
	/// 	}
	///
	/// 	#[pallet::config]
	/// 	pub trait Config: frame_system::Config {}
	/// }
	/// ```
	/// I.e. a regular trait implementation with generic bound: `T: Config`, for the trait
	/// `Hooks<BlockNumberFor<T>>` (they are defined in preludes), for the type `Pallet<T>`.
	///
	/// Optionally, you could add a where clause.
	///
	/// ## Macro expansion
	///
	/// The macro implements the traits
	/// [`OnInitialize`](frame_support::traits::OnInitialize),
	/// [`OnIdle`](frame_support::traits::OnIdle),
	/// [`OnFinalize`](frame_support::traits::OnFinalize),
	/// [`OnRuntimeUpgrade`](frame_support::traits::OnRuntimeUpgrade),
	/// [`OffchainWorker`](frame_support::traits::OffchainWorker), and
	/// [`IntegrityTest`](frame_support::traits::IntegrityTest) using
	/// the provided [`Hooks`](frame_support::traits::Hooks) implementation.
	///
	/// NOTE: `OnRuntimeUpgrade` is implemented with `Hooks::on_runtime_upgrade` and some
	/// additional logic. E.g. logic to write the pallet version into storage.
	///
	/// NOTE: The macro also adds some tracing logic when implementing the above traits. The
	/// following hooks emit traces: `on_initialize`, `on_finalize` and `on_runtime_upgrade`.
	pub use frame_support_procedural::hooks;

	/// The attribute `#[pallet::generate_deposit($visibility fn deposit_event)]` generates a
	/// helper function on `Pallet` that handles deposit events.
	///
	/// NOTE: For instantiable pallets, the event must be generic over `T` and `I`.
	///
	/// ## Macro expansion
	///
	/// The macro will add on enum `Event` the attributes:
	/// * `#[derive(`[`frame_support::CloneNoBound`]`)]`
	/// * `#[derive(`[`frame_support::EqNoBound`]`)]`
	/// * `#[derive(`[`frame_support::PartialEqNoBound`]`)]`
	/// * `#[derive(`[`frame_support::RuntimeDebugNoBound`]`)]`
	/// * `#[derive(`[`codec::Encode`]`)]`
	/// * `#[derive(`[`codec::Decode`]`)]`
	///
	/// The macro implements `From<Event<..>>` for ().
	///
	/// The macro implements a metadata function on `Event` returning the `EventMetadata`.
	///
	/// If `#[pallet::generate_deposit]` is present then the macro implements `fn
	/// deposit_event` on `Pallet`.
	pub use frame_support_procedural::generate_deposit;

	/// Each dispatchable may be annotated with the `#[pallet::feeless_if($closure)]`
	/// attribute, which explicitly defines the condition for the dispatchable to be feeless.
	///
	/// The arguments for the closure must be the referenced arguments of the dispatchable
	/// function.
	///
	/// The closure must return `bool`.
	///
	/// ### Example
	///
	/// ```
	/// #[frame_support::pallet(dev_mode)]
	/// mod pallet {
	/// 	use frame_support::pallet_prelude::*;
	/// 	use frame_system::pallet_prelude::*;
	///
	/// 	#[pallet::pallet]
	/// 	pub struct Pallet<T>(_);
	///
	/// 	#[pallet::call]
	/// 	impl<T: Config> Pallet<T> {
	/// 		#[pallet::call_index(0)]
	/// 		/// Marks this call as feeless if `foo` is zero.
	/// 		#[pallet::feeless_if(|_origin: &OriginFor<T>, foo: &u32| -> bool {
	/// 			*foo == 0
	/// 		})]
	/// 		pub fn something(
	/// 			_: OriginFor<T>,
	/// 			foo: u32,
	/// 		) -> DispatchResult {
	/// 			unimplemented!()
	/// 		}
	/// 	}
	///
	/// 	#[pallet::config]
	/// 	pub trait Config: frame_system::Config {}
	/// }
	/// ```
	///
	/// Please note that this only works for signed dispatchables and requires a signed
	/// extension such as [`pallet_skip_feeless_payment::SkipCheckIfFeeless`] to wrap the
	/// existing payment extension. Else, this is completely ignored and the dispatchable is
	/// still charged.
	///
	/// ### Macro expansion
	///
	/// The macro implements the [`pallet_skip_feeless_payment::CheckIfFeeless`] trait on the
	/// dispatchable and calls the corresponding closure in the implementation.
	///
	/// [`pallet_skip_feeless_payment::SkipCheckIfFeeless`]: ../../pallet_skip_feeless_payment/struct.SkipCheckIfFeeless.html
	/// [`pallet_skip_feeless_payment::CheckIfFeeless`]: ../../pallet_skip_feeless_payment/struct.SkipCheckIfFeeless.html
	pub use frame_support_procedural::feeless_if;

	/// The `#[pallet::error]` attribute allows you to define an error enum that will be
	/// returned from the dispatchable when an error occurs. The information for this error
	/// type is then stored in metadata.
	///
	/// Item must be defined as so:
	///
	/// ```
	/// #[frame_support::pallet(dev_mode)]
	/// mod pallet {
	/// 	#[pallet::pallet]
	/// 	pub struct Pallet<T>(_);
	///
	/// 	#[pallet::error]
	/// 	pub enum Error<T> {
	/// 		/// SomeFieldLessVariant doc
	/// 		SomeFieldLessVariant,
	/// 		/// SomeVariantWithOneField doc
	/// 		SomeVariantWithOneField(u32),
	/// 	}
	///
	/// 	#[pallet::config]
	/// 	pub trait Config: frame_system::Config {}
	/// }
	/// ```
	/// I.e. a regular enum named `Error`, with generic `T` and fieldless or multiple-field
	/// variants.
	///
	/// Any field type in the enum variants must implement [`scale_info::TypeInfo`] in order to
	/// be properly used in the metadata, and its encoded size should be as small as possible,
	/// preferably 1 byte in size in order to reduce storage size. The error enum itself has an
	/// absolute maximum encoded size specified by
	/// [`frame_support::MAX_MODULE_ERROR_ENCODED_SIZE`].
	///
	/// (1 byte can still be 256 different errors. The more specific the error, the easier it
	/// is to diagnose problems and give a better experience to the user. Don't skimp on having
	/// lots of individual error conditions.)
	///
	/// Field types in enum variants must also implement [`frame_support::PalletError`],
	/// otherwise the pallet will fail to compile. Rust primitive types have already
	/// implemented the [`frame_support::PalletError`] trait along with some commonly used
	/// stdlib types such as [`Option`] and [`sp_std::marker::PhantomData`], and hence
	/// in most use cases, a manual implementation is not necessary and is discouraged.
	///
	/// The generic `T` must not bound anything and a `where` clause is not allowed. That said,
	/// bounds and/or a where clause should not needed for any use-case.
	///
	/// ## Macro expansion
	///
	/// The macro implements the [`Debug`] trait and functions `as_u8` using variant position,
	/// and `as_str` using variant doc.
	///
	/// The macro also implements `From<Error<T>>` for `&'static str` and `From<Error<T>>` for
	/// `DispatchError`.
	pub use frame_support_procedural::error;

	/// The `#[pallet::event]` attribute allows you to define pallet events. Pallet events are
	/// stored under the `system` / `events` key when the block is applied (and then replaced
	/// when the next block writes it's events).
	///
	/// The Event enum can be defined as follows:
	///
	/// ```
	/// #[frame_support::pallet(dev_mode)]
	/// mod pallet {
	///     use frame_support::pallet_prelude::IsType;
	///
	/// 	#[pallet::pallet]
	/// 	pub struct Pallet<T>(_);
	///
	/// 	#[pallet::event]
	/// 	#[pallet::generate_deposit(fn deposit_event)] // Optional
	/// 	pub enum Event<T> {
	/// 		/// SomeEvent doc
	/// 		SomeEvent(u16, u32), // SomeEvent with two fields
	/// 	}
	///
	/// 	#[pallet::config]
	/// 	pub trait Config: frame_system::Config {
	/// 		/// The overarching runtime event type.
	/// 		type RuntimeEvent: From<Event<Self>>
	/// 			+ IsType<<Self as frame_system::Config>::RuntimeEvent>;
	/// 	}
	/// }
	/// ```
	///
	/// I.e. an enum (with named or unnamed fields variant), named `Event`, with generic: none
	/// or `T` or `T: Config`, and optional w here clause.
	///
	/// `RuntimeEvent` must be defined in the `Config`, as shown in the example.
	///
	/// Each field must implement [`Clone`], [`Eq`], [`PartialEq`], [`codec::Encode`],
	/// [`codec::Decode`], and [`Debug`] (on std only). For ease of use, bound by the trait
	/// `Member`, available in `frame_support::pallet_prelude`.
	pub use frame_support_procedural::event;

	/// Allows a pallet to declare a set of functions as a *dispatchable extrinsic*. In
	/// slightly simplified terms, this macro declares the set of "transactions" of a pallet.
	///
	/// > The exact definition of **extrinsic** can be found in
	/// > [`sp_runtime::generic::UncheckedExtrinsic`].
	///
	/// A **dispatchable** is a common term in FRAME, referring to process of constructing a
	/// function, and dispatching it with the correct inputs. This is commonly used with
	/// extrinsics, for example "an extrinsic has been dispatched". See
	/// [`sp_runtime::traits::Dispatchable`] and [`crate::traits::UnfilteredDispatchable`].
	///
	/// ## Call Enum
	///
	/// The macro is called `call` (rather than `#[pallet::extrinsics]`) because of the
	/// generation of a `enum Call`. This enum contains only the encoding of the function
	/// arguments of the dispatchable, alongside the information needed to route it to the
	/// correct function.
	///
	/// ```
	/// #[frame_support::pallet(dev_mode)]
	/// pub mod custom_pallet {
	/// #   use frame_support::pallet_prelude::*;
	/// #   use frame_system::pallet_prelude::*;
	/// #   #[pallet::config]
	/// #   pub trait Config: frame_system::Config {}
	/// #   #[pallet::pallet]
	/// #   pub struct Pallet<T>(_);
	/// #   use frame_support::traits::BuildGenesisConfig;
	///     #[pallet::call]
	///     impl<T: Config> Pallet<T> {
	///         pub fn some_dispatchable(_origin: OriginFor<T>, _input: u32) -> DispatchResult {
	///             Ok(())
	///         }
	///         pub fn other(_origin: OriginFor<T>, _input: u64) -> DispatchResult {
	///             Ok(())
	///         }
	///     }
	///
	///     // generates something like:
	///     // enum Call<T: Config> {
	///     //  some_dispatchable { input: u32 }
	///     //  other { input: u64 }
	///     // }
	/// }
	///
	/// fn main() {
	/// #   use frame_support::{derive_impl, construct_runtime};
	/// #   use frame_support::__private::codec::Encode;
	/// #   use frame_support::__private::TestExternalities;
	/// #   use frame_support::traits::UnfilteredDispatchable;
	/// #    impl custom_pallet::Config for Runtime {}
	/// #    #[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
	/// #    impl frame_system::Config for Runtime {
	/// #        type Block = frame_system::mocking::MockBlock<Self>;
	/// #    }
	///     construct_runtime! {
	///         pub struct Runtime {
	///             System: frame_system,
	///             Custom: custom_pallet
	///         }
	///     }
	///
	/// #    TestExternalities::new_empty().execute_with(|| {
	///     let origin: RuntimeOrigin = frame_system::RawOrigin::Signed(10).into();
	///     // calling into a dispatchable from within the runtime is simply a function call.
	///         let _ = custom_pallet::Pallet::<Runtime>::some_dispatchable(origin.clone(), 10);
	///
	///     // calling into a dispatchable from the outer world involves constructing the bytes of
	///     let call = custom_pallet::Call::<Runtime>::some_dispatchable { input: 10 };
	///     let _ = call.clone().dispatch_bypass_filter(origin);
	///
	///     // the routing of a dispatchable is simply done through encoding of the `Call` enum,
	///     // which is the index of the variant, followed by the arguments.
	///     assert_eq!(call.encode(), vec![0u8, 10, 0, 0, 0]);
	///
	///     // notice how in the encoding of the second function, the first byte is different and
	///     // referring to the second variant of `enum Call`.
	///     let call = custom_pallet::Call::<Runtime>::other { input: 10 };
	///     assert_eq!(call.encode(), vec![1u8, 10, 0, 0, 0, 0, 0, 0, 0]);
	///     #    });
	/// }
	/// ```
	///
	/// Further properties of dispatchable functions are as follows:
	///
	/// - Unless if annotated by `dev_mode`, it must contain [`weight`] to denote the
	///   pre-dispatch weight consumed.
	/// - The dispatchable must declare its index via [`call_index`], which can override the
	///   position of a function in `enum Call`.
	/// - The first argument is always an `OriginFor` (or `T::RuntimeOrigin`).
	/// - The return type is always [`crate::dispatch::DispatchResult`] (or
	///   [`crate::dispatch::DispatchResultWithPostInfo`]).
	///
	/// **WARNING**: modifying dispatchables, changing their order (i.e. using [`call_index`]),
	/// removing some, etc., must be done with care. This will change the encoding of the , and
	/// the call can be stored on-chain (e.g. in `pallet-scheduler`). Thus, migration might be
	/// needed. This is why the use of `call_index` is mandatory by default in FRAME.
	///
	/// ## Default Behavior
	///
	/// If no `#[pallet::call]` exists, then a default implementation corresponding to the
	/// following code is automatically generated:
	///
	/// ```
	/// #[frame_support::pallet(dev_mode)]
	/// mod pallet {
	/// 	#[pallet::pallet]
	/// 	pub struct Pallet<T>(_);
	///
	/// 	#[pallet::call] // <- automatically generated
	/// 	impl<T: Config> Pallet<T> {} // <- automatically generated
	///
	/// 	#[pallet::config]
	/// 	pub trait Config: frame_system::Config {}
	/// }
	/// ```
	pub use frame_support_procedural::call;

	/// Enforce the index of a variant in the generated `enum Call`. See [`call`] for more
	/// information.
	///
	/// All call indexes start from 0, until it encounters a dispatchable function with a
	/// defined call index. The dispatchable function that lexically follows the function with
	/// a defined call index will have that call index, but incremented by 1, e.g. if there are
	/// 3 dispatchable functions `fn foo`, `fn bar` and `fn qux` in that order, and only `fn
	/// bar` has a call index of 10, then `fn qux` will have an index of 11, instead of 1.
	pub use frame_support_procedural::call_index;

	/// Declares the arguments of a [`call`] function to be encoded using
	/// [`codec::Compact`]. This will results in smaller extrinsic encoding.
	///
	/// A common example of `compact` is for numeric values that are often times far far away
	/// from their theoretical maximum. For example, in the context of a crypto-currency, the
	/// balance of an individual account is oftentimes way less than what the numeric type
	/// allows. In all such cases, using `compact` is sensible.
	///
	/// ```
	/// #[frame_support::pallet(dev_mode)]
	/// pub mod custom_pallet {
	/// #   use frame_support::pallet_prelude::*;
	/// #   use frame_system::pallet_prelude::*;
	/// #   #[pallet::config]
	/// #   pub trait Config: frame_system::Config {}
	/// #   #[pallet::pallet]
	/// #   pub struct Pallet<T>(_);
	/// #   use frame_support::traits::BuildGenesisConfig;
	///     #[pallet::call]
	///     impl<T: Config> Pallet<T> {
	///         pub fn some_dispatchable(_origin: OriginFor<T>, #[pallet::compact] _input: u32) -> DispatchResult {
	///             Ok(())
	///         }
	///     }
	/// }
	pub use frame_support_procedural::compact;

	/// Allows you to define the genesis configuration for the pallet.
	///
	/// Item is defined as either an enum or a struct. It needs to be public and implement the
	/// trait [`frame_support::traits::BuildGenesisConfig`].
	///
	/// See [`genesis_build`] for an example.
	pub use frame_support_procedural::genesis_config;

	/// Allows you to define how the state of your pallet at genesis is built. This
	/// takes as input the `GenesisConfig` type (as `self`) and constructs the pallet's initial
	/// state.
	///
	/// The fields of the `GenesisConfig` can in turn be populated by the chain-spec.
	///
	/// ## Example
	///
	/// ```
	/// #[frame_support::pallet]
	/// pub mod pallet {
	/// # 	#[pallet::config]
	/// # 	pub trait Config: frame_system::Config {}
	/// # 	#[pallet::pallet]
	/// # 	pub struct Pallet<T>(_);
	/// # 	use frame_support::traits::BuildGenesisConfig;
	///     #[pallet::genesis_config]
	///     #[derive(frame_support::DefaultNoBound)]
	///     pub struct GenesisConfig<T: Config> {
	///         foo: Vec<T::AccountId>
	///     }
	///
	///     #[pallet::genesis_build]
	///     impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
	///         fn build(&self) {
	///             // use &self to access fields.
	///             let foo = &self.foo;
	///             todo!()
	///         }
	///     }
	/// }
	/// ```
	///
	/// ## Former Usage
	///
	/// Prior to <https://github.com/paritytech/substrate/pull/14306>, the following syntax was used.
	/// This is deprecated and will soon be removed.
	///
	/// ```
	/// #[frame_support::pallet]
	/// pub mod pallet {
	/// #     #[pallet::config]
	/// #     pub trait Config: frame_system::Config {}
	/// #     #[pallet::pallet]
	/// #     pub struct Pallet<T>(_);
	/// #     use frame_support::traits::GenesisBuild;
	///     #[pallet::genesis_config]
	///     #[derive(frame_support::DefaultNoBound)]
	///     pub struct GenesisConfig<T: Config> {
	/// 		foo: Vec<T::AccountId>
	/// 	}
	///
	///     #[pallet::genesis_build]
	///     impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
	///         fn build(&self) {
	///             todo!()
	///         }
	///     }
	/// }
	/// ```
	pub use frame_support_procedural::genesis_build;

	/// The `#[pallet::constant]` attribute can be used to add an associated type trait bounded
	/// by [`Get`](frame_support::pallet_prelude::Get) from [`pallet::config`](`macro@config`)
	/// into metadata.
	///
	/// ## Example
	///
	/// ```
	/// #[frame_support::pallet]
	/// mod pallet {
	///     use frame_support::pallet_prelude::*;
	///     # #[pallet::pallet]
	///     # pub struct Pallet<T>(_);
	///     #[pallet::config]
	///     pub trait Config: frame_system::Config {
	/// 		/// This is like a normal `Get` trait, but it will be added into metadata.
	/// 		#[pallet::constant]
	/// 		type Foo: Get<u32>;
	/// 	}
	/// }
	/// ```
	pub use frame_support_procedural::constant;

	/// Declares a type alias as a storage item. Storage items are pointers to data stored
	/// on-chain (the *blockchain state*), under a specific key. The exact key is dependent on
	/// the type of the storage.
	///
	/// > From the perspective of this pallet, the entire blockchain state is abstracted behind
	/// > a key-value api, namely [`sp_io::storage`].
	///
	/// ## Storage Types
	///
	/// The following storage types are supported by the `#[storage]` macro. For specific
	/// information about each storage type, refer to the documentation of the respective type.
	///
	/// * [`StorageValue`](crate::storage::types::StorageValue)
	/// * [`StorageMap`](crate::storage::types::StorageMap)
	/// * [`CountedStorageMap`](crate::storage::types::CountedStorageMap)
	/// * [`StorageDoubleMap`](crate::storage::types::StorageDoubleMap)
	/// * [`StorageNMap`](crate::storage::types::StorageNMap)
	/// * [`CountedStorageNMap`](crate::storage::types::CountedStorageNMap)
	///
	/// ## Storage Type Usage
	///
	/// The following details are relevant to all of the aforementioned storage types.
	/// Depending on the exact storage type, it may require the following generic parameters:
	///
	/// * [`Prefix`](#prefixes) - Used to give the storage item a unique key in the underlying
	///   storage.
	/// * `Key` - Type of the keys used to store the values,
	/// * `Value` - Type of the value being stored,
	/// * [`Hasher`](#hashers) - Used to ensure the keys of a map are uniformly distributed,
	/// * [`QueryKind`](#querykind) - Used to configure how to handle queries to the underlying
	///   storage,
	/// * `OnEmpty` - Used to handle missing values when querying the underlying storage,
	/// * `MaxValues` - _not currently used_.
	///
	/// Each `Key` type requires its own designated `Hasher` declaration, so that
	/// [`StorageDoubleMap`](frame_support::storage::types::StorageDoubleMap) needs two of
	/// each, and [`StorageNMap`](frame_support::storage::types::StorageNMap) needs `N` such
	/// pairs. Since [`StorageValue`](frame_support::storage::types::StorageValue) only stores
	/// a single element, no configuration of hashers is needed.
	///
	/// ### Syntax
	///
	/// Two general syntaxes are supported, as demonstrated below:
	///
	/// 1. Named type parameters, e.g., `type Foo<T> = StorageValue<Value = u32>`.
	/// 2. Positional type parameters, e.g., `type Foo<T> = StorageValue<_, u32>`.
	///
	/// In both instances, declaring the generic parameter `<T>` is mandatory. Optionally, it
	/// can also be explicitly declared as `<T: Config>`. In the compiled code, `T` will
	/// automatically include the trait bound `Config`.
	///
	/// Note that in positional syntax, the first generic type parameter must be `_`.
	///
	/// #### Example
	///
	/// ```
	/// #[frame_support::pallet]
	/// mod pallet {
	///     # use frame_support::pallet_prelude::*;
	///     # #[pallet::config]
	///     # pub trait Config: frame_system::Config {}
	///     # #[pallet::pallet]
	///     # pub struct Pallet<T>(_);
	///     /// Positional syntax, without bounding `T`.
	///     #[pallet::storage]
	///     pub type Foo<T> = StorageValue<_, u32>;
	///
	///     /// Positional syntax, with bounding `T`.
	///     #[pallet::storage]
	///     pub type Bar<T: Config> = StorageValue<_, u32>;
	///
	///     /// Named syntax.
	///     #[pallet::storage]
	///     pub type Baz<T> = StorageMap<Hasher = Blake2_128Concat, Key = u32, Value = u32>;
	/// }
	/// ```
	///
	/// ### QueryKind
	///
	/// Every storage type mentioned above has a generic type called
	/// [`QueryKind`](frame_support::storage::types::QueryKindTrait) that determines its
	/// "query" type. This refers to the kind of value returned when querying the storage, for
	/// instance, through a `::get()` method.
	///
	/// There are three types of queries:
	///
	/// 1. [`OptionQuery`](frame_support::storage::types::OptionQuery): The default query type.
	///    It returns `Some(V)` if the value is present, or `None` if it isn't, where `V` is
	///    the value type.
	/// 2. [`ValueQuery`](frame_support::storage::types::ValueQuery): Returns the value itself
	///    if present; otherwise, it returns `Default::default()`. This behavior can be
	///    adjusted with the `OnEmpty` generic parameter, which defaults to `OnEmpty =
	///    GetDefault`.
	/// 3. [`ResultQuery`](frame_support::storage::types::ResultQuery): Returns `Result<V, E>`,
	///    where `V` is the value type.
	///
	/// See [`QueryKind`](frame_support::storage::types::QueryKindTrait) for further examples.
	///
	/// ### Optimized Appending
	///
	/// All storage items — such as
	/// [`StorageValue`](frame_support::storage::types::StorageValue),
	/// [`StorageMap`](frame_support::storage::types::StorageMap), and their variants—offer an
	/// `::append()` method optimized for collections. Using this method avoids the
	/// inefficiency of decoding and re-encoding entire collections when adding items. For
	/// instance, consider the storage declaration `type MyVal<T> = StorageValue<_, Vec<u8>,
	/// ValueQuery>`. With `MyVal` storing a large list of bytes, `::append()` lets you
	/// directly add bytes to the end in storage without processing the full list. Depending on
	/// the storage type, additional key specifications may be needed.
	///
	/// #### Example
	#[doc = docify::embed!("src/lib.rs", example_storage_value_append)]
	/// Similarly, there also exists a `::try_append()` method, which can be used when handling
	/// types where an append operation might fail, such as a
	/// [`BoundedVec`](frame_support::BoundedVec).
	///
	/// #### Example
	#[doc = docify::embed!("src/lib.rs", example_storage_value_try_append)]
	/// ### Optimized Length Decoding
	///
	/// All storage items — such as
	/// [`StorageValue`](frame_support::storage::types::StorageValue),
	/// [`StorageMap`](frame_support::storage::types::StorageMap), and their counterparts —
	/// incorporate the `::decode_len()` method. This method allows for efficient retrieval of
	/// a collection's length without the necessity of decoding the entire dataset.
	/// #### Example
	#[doc = docify::embed!("src/lib.rs", example_storage_value_decode_len)]
	/// ### Hashers
	///
	/// For all storage types, except
	/// [`StorageValue`](frame_support::storage::types::StorageValue), a set of hashers needs
	/// to be specified. The choice of hashers is crucial, especially in production chains. The
	/// purpose of storage hashers in maps is to ensure the keys of a map are
	/// uniformly distributed. An unbalanced map/trie can lead to inefficient performance.
	///
	/// In general, hashers are categorized as either cryptographically secure or not. The
	/// former is slower than the latter. `Blake2` and `Twox` serve as examples of each,
	/// respectively.
	///
	/// As a rule of thumb:
	///
	/// 1. If the map keys are not controlled by end users, or are cryptographically secure by
	/// definition (e.g., `AccountId`), then the use of cryptographically secure hashers is NOT
	/// required.
	/// 2. If the map keys are controllable by the end users, cryptographically secure hashers
	/// should be used.
	///
	/// For more information, look at the types that implement
	/// [`frame_support::StorageHasher`](frame_support::StorageHasher).
	///
	/// Lastly, it's recommended for hashers with "concat" to have reversible hashes. Refer to
	/// the implementors section of
	/// [`hash::ReversibleStorageHasher`](frame_support::hash::ReversibleStorageHasher).
	///
	/// ### Prefixes
	///
	/// Internally, every storage type generates a "prefix". This prefix serves as the initial
	/// segment of the key utilized to store values in the on-chain state (i.e., the final key
	/// used in [`sp_io::storage`](sp_io::storage)). For all storage types, the following rule
	/// applies:
	///
	/// > The storage prefix begins with `twox128(pallet_prefix) ++ twox128(STORAGE_PREFIX)`,
	/// > where
	/// > `pallet_prefix` is the name assigned to the pallet instance in
	/// > [`frame_support::construct_runtime`](frame_support::construct_runtime), and
	/// > `STORAGE_PREFIX` is the name of the `type` aliased to a particular storage type, such
	/// > as
	/// > `Foo` in `type Foo<T> = StorageValue<..>`.
	///
	/// For [`StorageValue`](frame_support::storage::types::StorageValue), no additional key is
	/// required. For map types, the prefix is extended with one or more keys defined by the
	/// map.
	///
	/// #### Example
	#[doc = docify::embed!("src/lib.rs", example_storage_value_map_prefixes)]
	/// ## Related Macros
	///
	/// The following attribute macros can be used in conjunction with the `#[storage]` macro:
	///
	/// * [`macro@getter`]: Creates a custom getter function.
	/// * [`macro@storage_prefix`]: Overrides the default prefix of the storage item.
	/// * [`macro@unbounded`]: Declares the storage item as unbounded.
	///
	/// #### Example
	/// ```
	/// #[frame_support::pallet]
	/// mod pallet {
	///     # use frame_support::pallet_prelude::*;
	///     # #[pallet::config]
	///     # pub trait Config: frame_system::Config {}
	///     # #[pallet::pallet]
	///     # pub struct Pallet<T>(_);
	/// 	/// A kitchen-sink StorageValue, with all possible additional attributes.
	///     #[pallet::storage]
	/// 	#[pallet::getter(fn foo)]
	/// 	#[pallet::storage_prefix = "OtherFoo"]
	/// 	#[pallet::unbounded]
	///     pub type Foo<T> = StorageValue<_, u32, ValueQuery>;
	/// }
	/// ```
	pub use frame_support_procedural::storage;
	/// This attribute is attached to a function inside an `impl` block annoated with
	/// [`pallet::tasks_experimental`](`tasks_experimental`) to define the conditions for a
	/// given work item to be valid.
	///
	/// It takes a closure as input, which is then used to define the condition. The closure
	/// should have the same signature as the function it is attached to, except that it should
	/// return a `bool` instead.
	pub use frame_support_procedural::task_condition;
	/// This attribute is attached to a function inside an `impl` block annoated with
	/// [`pallet::tasks_experimental`](`tasks_experimental`) to define the index of a given
	/// work item.
	///
	/// It takes an integer literal as input, which is then used to define the index. This
	/// index should be unique for each function in the `impl` block.
	pub use frame_support_procedural::task_index;
	/// This attribute is attached to a function inside an `impl` block annoated with
	/// [`pallet::tasks_experimental`](`tasks_experimental`) to define an iterator over the
	/// available work items for a task.
	///
	/// It takes an iterator as input that yields a tuple with same types as the function
	/// arguments.
	pub use frame_support_procedural::task_list;
	/// This attribute is attached to a function inside an `impl` block annoated with
	/// [`pallet::tasks_experimental`](`tasks_experimental`) define the weight of a given work
	/// item.
	///
	/// It takes a closure as input, which should return a `Weight` value.
	pub use frame_support_procedural::task_weight;
	/// Allows you to define some service work that can be recognized by a script or an
	/// off-chain worker. Such a script can then create and submit all such work items at any
	/// given time.
	///
	/// These work items are defined as instances of the [`Task`](frame_support::traits::Task)
	/// trait. [`pallet:tasks_experimental`](`tasks_experimental`) when attached to an `impl`
	/// block inside a pallet, will generate an enum `Task<T>` whose variants are mapped to
	/// functions inside this `impl` block.
	///
	/// Each such function must have the following set of attributes:
	///
	/// * [`pallet::task_list`](`task_list`)
	/// * [`pallet::task_condition`](`task_condition`)
	/// * [`pallet::task_weight`](`task_weight`)
	/// * [`pallet::task_index`](`task_index`)
	///
	/// All of such Tasks are then aggregated into a `RuntimeTask` by
	/// [`construct_runtime`](frame_support::construct_runtime).
	///
	/// Finally, the `RuntimeTask` can then used by a script or off-chain worker to create and
	/// submit such tasks via an extrinsic defined in `frame_system` called `do_task`.
	///
	/// ## Example
	#[doc = docify::embed!("src/tests/tasks.rs", tasks_example)]
	/// Now, this can be executed as follows:
	#[doc = docify::embed!("src/tests/tasks.rs", tasks_work)]
	pub use frame_support_procedural::tasks_experimental;
}

#[deprecated(note = "Will be removed after July 2023; Use `sp_runtime::traits` directly instead.")]
pub mod error {
	#[doc(hidden)]
	pub use sp_runtime::traits::{BadOrigin, LookupError};
}

#[doc(inline)]
pub use frame_support_procedural::register_default_impl;

// Generate a macro that will enable/disable code based on `std` feature being active.
sp_core::generate_feature_enabled_macro!(std_enabled, feature = "std", $);

// Helper for implementing GenesisBuilder runtime API
pub mod genesis_builder_helper;

#[cfg(test)]
mod test {
	// use super::*;
	use crate::{
		hash::*,
		storage::types::{StorageMap, StorageValue, ValueQuery},
		traits::{ConstU32, StorageInstance},
		BoundedVec,
	};
	use sp_io::{hashing::twox_128, TestExternalities};

	struct Prefix;
	impl StorageInstance for Prefix {
		fn pallet_prefix() -> &'static str {
			"test"
		}
		const STORAGE_PREFIX: &'static str = "foo";
	}

	struct Prefix1;
	impl StorageInstance for Prefix1 {
		fn pallet_prefix() -> &'static str {
			"test"
		}
		const STORAGE_PREFIX: &'static str = "MyVal";
	}
	struct Prefix2;
	impl StorageInstance for Prefix2 {
		fn pallet_prefix() -> &'static str {
			"test"
		}
		const STORAGE_PREFIX: &'static str = "MyMap";
	}

	#[docify::export]
	#[test]
	pub fn example_storage_value_try_append() {
		type MyVal = StorageValue<Prefix, BoundedVec<u8, ConstU32<10>>, ValueQuery>;

		TestExternalities::default().execute_with(|| {
			MyVal::set(BoundedVec::try_from(vec![42, 43]).unwrap());
			assert_eq!(MyVal::get(), vec![42, 43]);
			// Try to append a single u32 to BoundedVec stored in `MyVal`
			assert_ok!(MyVal::try_append(40));
			assert_eq!(MyVal::get(), vec![42, 43, 40]);
		});
	}

	#[docify::export]
	#[test]
	pub fn example_storage_value_append() {
		type MyVal = StorageValue<Prefix, Vec<u8>, ValueQuery>;

		TestExternalities::default().execute_with(|| {
			MyVal::set(vec![42, 43]);
			assert_eq!(MyVal::get(), vec![42, 43]);
			// Append a single u32 to Vec stored in `MyVal`
			MyVal::append(40);
			assert_eq!(MyVal::get(), vec![42, 43, 40]);
		});
	}

	#[docify::export]
	#[test]
	pub fn example_storage_value_decode_len() {
		type MyVal = StorageValue<Prefix, BoundedVec<u8, ConstU32<10>>, ValueQuery>;

		TestExternalities::default().execute_with(|| {
			MyVal::set(BoundedVec::try_from(vec![42, 43]).unwrap());
			assert_eq!(MyVal::decode_len().unwrap(), 2);
		});
	}

	#[docify::export]
	#[test]
	pub fn example_storage_value_map_prefixes() {
		type MyVal = StorageValue<Prefix1, u32, ValueQuery>;
		type MyMap = StorageMap<Prefix2, Blake2_128Concat, u16, u32, ValueQuery>;
		TestExternalities::default().execute_with(|| {
			// This example assumes `pallet_prefix` to be "test"
			// Get storage key for `MyVal` StorageValue
			assert_eq!(
				MyVal::hashed_key().to_vec(),
				[twox_128(b"test"), twox_128(b"MyVal")].concat()
			);
			// Get storage key for `MyMap` StorageMap and `key` = 1
			let mut k: Vec<u8> = vec![];
			k.extend(&twox_128(b"test"));
			k.extend(&twox_128(b"MyMap"));
			k.extend(&1u16.blake2_128_concat());
			assert_eq!(MyMap::hashed_key_for(1).to_vec(), k);
		});
	}
}
