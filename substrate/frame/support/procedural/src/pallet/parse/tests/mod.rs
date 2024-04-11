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

use std::{panic, sync::Mutex};
use syn::parse_quote;

#[doc(hidden)]
pub mod __private {
	pub use regex;
}

/// Allows you to assert that the input expression resolves to an error whose string
/// representation matches the specified regex literal.
///
/// ## Example:
///
/// ```
/// use super::tasks::*;
///
/// assert_parse_error_matches!(
/// 	parse2::<TaskEnumDef>(quote! {
/// 		#[pallet::task_enum]
/// 		pub struct Something;
/// 	}),
/// 	"expected `enum`"
/// );
/// ```
///
/// More complex regular expressions are also possible (anything that could pass as a regex for
/// use with the [`regex`] crate.):
///
/// ```ignore
/// assert_parse_error_matches!(
/// 	parse2::<TasksDef>(quote! {
/// 		#[pallet::tasks_experimental]
/// 		impl<T: Config<I>, I: 'static> Pallet<T, I> {
/// 			#[pallet::task_condition(|i| i % 2 == 0)]
/// 			#[pallet::task_index(0)]
/// 			pub fn foo(i: u32) -> DispatchResult {
/// 				Ok(())
/// 			}
/// 		}
/// 	}),
/// 	r"missing `#\[pallet::task_list\(\.\.\)\]`"
/// );
/// ```
///
/// Although this is primarily intended to be used with parsing errors, this macro is general
/// enough that it will work with any error with a reasonable [`core::fmt::Display`] impl.
#[macro_export]
macro_rules! assert_parse_error_matches {
	($expr:expr, $reg:literal) => {
		match $expr {
			Ok(_) => panic!("Expected an `Error(..)`, but got Ok(..)"),
			Err(e) => {
				let error_message = e.to_string();
				let re = $crate::pallet::parse::tests::__private::regex::Regex::new($reg)
					.expect("Invalid regex pattern");
				assert!(
					re.is_match(&error_message),
					"Error message \"{}\" does not match the pattern \"{}\"",
					error_message,
					$reg
				);
			},
		}
	};
}

/// Allows you to assert that an entire pallet parses successfully. A custom syntax is used for
/// specifying arguments so please pay attention to the docs below.
///
/// The general syntax is:
///
/// ```ignore
/// assert_pallet_parses! {
/// 	#[manifest_dir("../../examples/basic")]
/// 	#[frame_support::pallet]
/// 	pub mod pallet {
/// 		#[pallet::config]
/// 		pub trait Config: frame_system::Config {}
///
/// 		#[pallet::pallet]
/// 		pub struct Pallet<T>(_);
/// 	}
/// };
/// ```
///
/// The `#[manifest_dir(..)]` attribute _must_ be specified as the _first_ attribute on the
/// pallet module, and should reference the relative (to your current directory) path of a
/// directory containing containing the `Cargo.toml` of a valid pallet. Typically you will only
/// ever need to use the `examples/basic` pallet, but sometimes it might be advantageous to
/// specify a different one that has additional dependencies.
///
/// The reason this must be specified is that our underlying parsing of pallets depends on
/// reaching out into the file system to look for particular `Cargo.toml` dependencies via the
/// [`generate_access_from_frame_or_crate`] method, so to simulate this properly in a proc
/// macro crate, we need to temporarily convince this function that we are running from the
/// directory of a valid pallet.
#[macro_export]
macro_rules! assert_pallet_parses {
	(
		#[manifest_dir($manifest_dir:literal)]
		$($tokens:tt)*
	) => {
		{
			let mut pallet: Option<$crate::pallet::parse::Def> = None;
			$crate::pallet::parse::tests::simulate_manifest_dir($manifest_dir, core::panic::AssertUnwindSafe(|| {
				pallet = Some($crate::pallet::parse::Def::try_from(syn::parse_quote! {
					$($tokens)*
				}, false).unwrap());
			}));
			pallet.unwrap()
		}
	}
}

/// Similar to [`assert_pallet_parses`], except this instead expects the pallet not to parse,
/// and allows you to specify a regex matching the expected parse error.
///
/// This is identical syntactically to [`assert_pallet_parses`] in every way except there is a
/// second attribute that must be specified immediately after `#[manifest_dir(..)]` which is
/// `#[error_regex(..)]` which should contain a string/regex literal designed to match what you
/// consider to be the correct parsing error we should see when we try to parse this particular
/// pallet.
///
/// ## Example:
///
/// ```
/// assert_pallet_parse_error! {
/// 	#[manifest_dir("../../examples/basic")]
/// 	#[error_regex("Missing `\\#\\[pallet::pallet\\]`")]
/// 	#[frame_support::pallet]
/// 	pub mod pallet {
/// 		#[pallet::config]
/// 		pub trait Config: frame_system::Config {}
/// 	}
/// }
/// ```
#[macro_export]
macro_rules! assert_pallet_parse_error {
	(
		#[manifest_dir($manifest_dir:literal)]
		#[error_regex($reg:literal)]
		$($tokens:tt)*
	) => {
		$crate::pallet::parse::tests::simulate_manifest_dir($manifest_dir, || {
			$crate::assert_parse_error_matches!(
				$crate::pallet::parse::Def::try_from(
					parse_quote! {
						$($tokens)*
					},
					false
				),
				$reg
			);
		});
	}
}

/// Safely runs the specified `closure` while simulating an alternative `CARGO_MANIFEST_DIR`,
/// restoring `CARGO_MANIFEST_DIR` to its original value upon completion regardless of whether
/// the closure panics.
///
/// This is useful in tests of `Def::try_from` and other pallet-related methods that internally
/// make use of [`generate_access_from_frame_or_crate`], which is sensitive to entries in the
/// "current" `Cargo.toml` files.
///
/// This function uses a [`Mutex`] to avoid a race condition created when multiple tests try to
/// modify and then restore the `CARGO_MANIFEST_DIR` ENV var in an overlapping way.
pub fn simulate_manifest_dir<P: AsRef<std::path::Path>, F: FnOnce() + std::panic::UnwindSafe>(
	path: P,
	closure: F,
) {
	use std::{env::*, path::*};

	/// Ensures that only one thread can modify/restore the `CARGO_MANIFEST_DIR` ENV var at a time,
	/// avoiding a race condition because `cargo test` runs tests in parallel.
	///
	/// Although this forces all tests that use [`simulate_manifest_dir`] to run sequentially with
	/// respect to each other, this is still several orders of magnitude faster than using UI
	/// tests, even if they are run in parallel.
	static MANIFEST_DIR_LOCK: Mutex<()> = Mutex::new(());

	// avoid race condition when swapping out `CARGO_MANIFEST_DIR`
	let guard = MANIFEST_DIR_LOCK.lock().unwrap();

	// obtain the current/original `CARGO_MANIFEST_DIR`
	let orig = PathBuf::from(
		var("CARGO_MANIFEST_DIR").expect("failed to read ENV var `CARGO_MANIFEST_DIR`"),
	);

	// set `CARGO_MANIFEST_DIR` to the provided path, relative to current working dir
	set_var("CARGO_MANIFEST_DIR", orig.join(path.as_ref()));

	// safely run closure catching any panics
	let result = panic::catch_unwind(closure);

	// restore original `CARGO_MANIFEST_DIR` before unwinding
	set_var("CARGO_MANIFEST_DIR", &orig);

	// unlock the mutex so we don't poison it if there is a panic
	drop(guard);

	// unwind any panics originally encountered when running closure
	result.unwrap();
}

mod tasks;

#[test]
fn test_parse_minimal_pallet() {
	assert_pallet_parses! {
		#[manifest_dir("../../examples/basic")]
		#[frame_support::pallet]
		pub mod pallet {
			#[pallet::config]
			pub trait Config: frame_system::Config {}

			#[pallet::pallet]
			pub struct Pallet<T>(_);
		}
	};
}

#[test]
fn test_parse_pallet_missing_pallet() {
	assert_pallet_parse_error! {
		#[manifest_dir("../../examples/basic")]
		#[error_regex("Missing `\\#\\[pallet::pallet\\]`")]
		#[frame_support::pallet]
		pub mod pallet {
			#[pallet::config]
			pub trait Config: frame_system::Config {}
		}
	}
}

#[test]
fn test_parse_pallet_missing_config() {
	assert_pallet_parse_error! {
		#[manifest_dir("../../examples/basic")]
		#[error_regex("Missing `\\#\\[pallet::config\\]`")]
		#[frame_support::pallet]
		pub mod pallet {
			#[pallet::pallet]
			pub struct Pallet<T>(_);
		}
	}
}
