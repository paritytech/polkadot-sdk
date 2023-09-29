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
use syn::parse_quote;

#[macro_export]
macro_rules! assert_error_matches {
	($expr:expr, $reg:literal) => {
		match $expr {
			Ok(_) => panic!("Expected an `Error(..)`, but got Ok(..)"),
			Err(e) => {
				let error_message = e.to_string();
				let re = regex::Regex::new($reg).expect("Invalid regex pattern");
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

#[macro_export]
macro_rules! assert_pallet_parses {
	(
		#[manifest_dir($manifest_dir:literal)]
		$($tokens:tt)*
	) => {
		$crate::pallet::parse::tests::simulate_manifest_dir($manifest_dir, || {
			$crate::pallet::parse::Def::try_from(syn::parse_quote! {
				$($tokens)*
			}, false).unwrap();
		});
	}
}

#[macro_export]
macro_rules! assert_pallet_parse_error {
	(
		#[manifest_dir($manifest_dir:literal)]
		#[error_regex($reg:literal)]
		$($tokens:tt)*
	) => {
		$crate::pallet::parse::tests::simulate_manifest_dir($manifest_dir, || {
			$crate::assert_error_matches!(
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

/// Implementation detail of [`simulate_manifest_dir`] that allows us to safely run a closure
/// under an alternative `CARGO_MANIFEST_DIR` such that it will always be set back to the
/// original value even if the closure panics.
struct ManifestContext<P: AsRef<std::path::Path>, F: FnMut()> {
	path: P,
	closure: F,
	orig: Option<std::path::PathBuf>,
}

impl<P: AsRef<std::path::Path>, F: FnMut()> ManifestContext<P, F> {
	fn run(&mut self) {
		use std::{env::*, path::*};

		// obtain the current/original `CARGO_MANIFEST_DIR`
		let orig = PathBuf::from(
			var("CARGO_MANIFEST_DIR").expect("failed to read ENV var `CARGO_MANIFEST_DIR`"),
		);

		// set `CARGO_MANIFEST_DIR` to the provided path, relative to current working dir
		set_var("CARGO_MANIFEST_DIR", orig.join(self.path.as_ref()));

		// cache the original `CARGO_MANIFEST_DIR` on this context
		self.orig = Some(orig);

		// run the closure
		(self.closure)();
	}
}

impl<P: AsRef<std::path::Path>, F: FnMut()> Drop for ManifestContext<P, F> {
	fn drop(&mut self) {
		let Some(orig) = &self.orig else { unreachable!() };
		// ensures that `CARGO_MANIFEST_DIR` is set back to its original value even if closure()
		// panicked or had a failed assertion.
		std::env::set_var("CARGO_MANIFEST_DIR", orig);
	}
}

/// Safely runs the specified `closure` while simulating an alternative
/// `CARGO_MANIFEST_DIR`, restoring `CARGO_MANIFEST_DIR` to its original value upon completion
/// regardless of whether the closure panics.
///
/// This useful in tests of `Def::try_from` and other pallet-related methods that internally
/// make use of [`generate_crate_access_2018`], which is sensitive to entries in the "current"
/// `Cargo.toml` files.
pub fn simulate_manifest_dir<P: AsRef<std::path::Path>, F: FnMut()>(path: P, closure: F) {
	let mut context = ManifestContext { path, closure, orig: None };
	context.run();
}

mod tasks;

#[test]
fn test_parse_minimal_pallet() {
	assert_pallet_parses!(
		#[manifest_dir("../../examples/basic")]
		#[frame_support::pallet]
		pub mod pallet {
			#[pallet::config]
			pub trait Config: frame_system::Config {}

			#[pallet::pallet]
			pub struct Pallet<T>(_);
		}
	);
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
