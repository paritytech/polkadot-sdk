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

//! Macros for the FRAME support library.

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
			< $($ty_params),* >( $(core::marker::PhantomData<$ty_params>),* )
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
			< $($ty_params),* >( $(core::marker::PhantomData<$ty_params>),* )
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
			< $($ty_params),* >( $(core::marker::PhantomData<$ty_params>),* )
		)?;
		$crate::parameter_types!(IMPL_STORAGE $name, $type, $value $( $(, $ty_params)* )?);
		$crate::parameter_types!( $( $rest )* );
	);
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
				$crate::__private::sp_crypto_hashing_proc_macro::twox_128!(b":", $name, b":")
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

					/// Kill/reset the value to whatever was set at first.
					#[allow(unused)]
					pub fn reset() {
						Self::set($value);
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
			fn sorted_members() -> $crate::__private::Vec<$type> { vec![$value] }
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
			let mut msg = $crate::__private::String::default();
			let _ = core::write!(&mut msg, $($arg)+);
			$crate::__private::sp_io::misc::print_utf8(msg.as_bytes())
		}
	}
}

/// Return Err of the expression: `return Err($expression);`.
///
/// Used as `fail!(expression)`.
#[macro_export]
macro_rules! fail {
	( $y:expr ) => {{
		return Err($y.into());
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
			::core::panic!("expected Err(_), got Ok(_).")
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
/// [`MAX_MODULE_ERROR_ENCODED_SIZE`](crate::MAX_MODULE_ERROR_ENCODED_SIZE) during compilation.
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
		#[allow(deprecated)]
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
macro_rules! hypothetically {
	( $e:expr ) => {
		$crate::storage::transactional::with_transaction(|| -> $crate::__private::TransactionOutcome<::core::result::Result<_, $crate::__private::DispatchError>> {
			$crate::__private::TransactionOutcome::Rollback(::core::result::Result::Ok($e))
		},
		).expect("Always returning Ok; qed")
	};
}

/// Assert something to be *hypothetically* `Ok`, without actually committing it.
///
/// Reverts any storage changes made by the closure.
#[macro_export]
macro_rules! hypothetically_ok {
	($e:expr $(, $args:expr)* $(,)?) => {
		$crate::assert_ok!($crate::hypothetically!($e) $(, $args)*);
	};
}

/// Puts the [`impl_for_tuples`](impl_trait_for_tuples::impl_for_tuples) attribute above the given
/// code.
///
/// The main purpose of this macro is to handle the `tuples-*` feature which informs the attribute
/// about the maximum size of the tuple to generate. Besides that, there is no difference to use the
/// attribute directly.
///
/// # Example
///
/// ```rust
/// trait ILoveTuples {
///     fn really_hard();
/// }
///
/// frame_support::impl_for_tuples_attr! {
///     impl ILoveTuples for Tuple {
///         fn really_hard() {
///             for_tuples! { #(
///                 // Print it for each tuple
///                 println!("I LOVE TUPLES");
///             )* }
///         }
///     }
/// }
/// ```
#[cfg(all(not(feature = "tuples-96"), not(feature = "tuples-128")))]
#[macro_export]
macro_rules! impl_for_tuples_attr {
	( $( $input:tt )* ) => {
		#[$crate::__private::impl_trait_for_tuples::impl_for_tuples(64)]
		$( $input )*
	}
}

/// Puts the [`impl_for_tuples`](impl_trait_for_tuples::impl_for_tuples) attribute above the given
/// code.
///
/// The main purpose of this macro is to handle the `tuples-*` feature which informs the attribute
/// about the maximum size of the tuple to generate. Besides that, there is no difference to use the
/// attribute directly.
///
/// # Example
///
/// ```rust
/// trait ILoveTuples {
///     fn really_hard();
/// }
///
/// frame_support::impl_for_tuples_attr! {
///     impl ILoveTuples for Tuple {
///         fn really_hard() {
///             for_tuples! { #(
///                 // Print it for each tuple
///                 println!("I LOVE TUPLES");
///             )* }
///         }
///     }
/// }
/// ```
#[cfg(all(feature = "tuples-96", not(feature = "tuples-128")))]
#[macro_export]
macro_rules! impl_for_tuples_attr {
	( $( $input:tt )* ) => {
		#[$crate::__private::impl_trait_for_tuples::impl_for_tuples(96)]
		$( $input )*
	}
}

/// Puts the [`impl_for_tuples`](impl_trait_for_tuples::impl_for_tuples) attribute above the given
/// code.
///
/// The main purpose of this macro is to handle the `tuples-*` feature which informs the attribute
/// about the maximum size of the tuple to generate. Besides that, there is no difference to use the
/// attribute directly.
///
/// # Example
///
/// ```rust
/// trait ILoveTuples {
///     fn really_hard();
/// }
///
/// frame_support::impl_for_tuples_attr! {
///     impl ILoveTuples for Tuple {
///         fn really_hard() {
///             for_tuples! { #(
///                 // Print it for each tuple
///                 println!("I LOVE TUPLES");
///             )* }
///         }
///     }
/// }
/// ```
#[cfg(feature = "tuples-128")]
#[macro_export]
macro_rules! impl_for_tuples_attr {
	( $( $input:tt )* ) => {
		#[$crate::__private::impl_trait_for_tuples::impl_for_tuples(128)]
		$( $input )*
	}
}
