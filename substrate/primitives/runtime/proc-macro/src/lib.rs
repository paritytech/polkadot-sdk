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

//! Proc macro for sp-runtime crate.

mod replace_features;

use replace_features::{ConfigurationPredicate, RuntimeFeature};

// NOTE: Those macro are only provided in attribute macro style because implementing a proc_macro
// which generates `#[cfg(feature = "some_features")]` is invalid, the compiler requires an item
// after the attribute in the generated code.
// `#![..]` cfg are also not supported because it is invalid to generate a `#![..]` macro call from
// inside a macro.

// NOTE: This documentation is shown on the re-exported macro in the sp-runtime crate.
// This doc is duplicated for each macro.
/// Extended implementation of `cfg` macro with access to `sp-runtime/try-runtime` and
/// `sp-runtime/runtime-benchmarks` features.
///
/// Syntax is the same as `cfg` macro.
///
/// `#![..]` style macro call are not supported.
///
/// # Example
///
/// ```
/// #[sp_runtime::runtime_cfg(feature = "sp-runtime/try-runtime")]
/// fn some_function() {
/// 	println!("try-runtime is enabled");
/// }
///
/// #[sp_runtime::runtime_cfg(all(feature = "std", feature = "sp-runtime/runtime-benchmarks"))]
/// fn some_function() {
/// 	println!("try-runtime and std are enabled");
/// }
/// ```
#[proc_macro_attribute]
pub fn with_features_try_runtime_and_runtime_benchmarks(
	args: proc_macro::TokenStream,
	input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
	let features = [
		RuntimeFeature { name: "sp-runtime/try-runtime".into(), is_enabled: true },
		RuntimeFeature { name: "sp-runtime/runtime-benchmarks".into(), is_enabled: true },
	];

	let mut args = syn::parse_macro_input!(args as ConfigurationPredicate);
	let input = proc_macro2::TokenStream::from(input);

	args.replace_features(&features[..]);

	quote::quote!(
		#[cfg(#args)]
		#input
	)
	.into()
}

// NOTE: This documentation is shown on the re-exported macro in the sp-runtime crate.
// This doc is duplicated for each macro.
/// Extended implementation of `cfg` macro with access to `sp-runtime/try-runtime` and
/// `sp-runtime/runtime-benchmarks` features.
///
/// Syntax is the same as `cfg` macro.
///
/// `#![..]` style macro call are not supported.
///
/// # Example
///
/// ```
/// #[sp_runtime::runtime_cfg(feature = "sp-runtime/try-runtime")]
/// fn some_function() {
/// 	println!("try-runtime is enabled");
/// }
///
/// #[sp_runtime::runtime_cfg(all(feature = "std", feature = "sp-runtime/runtime-benchmarks"))]
/// fn some_function() {
/// 	println!("try-runtime and std are enabled");
/// }
/// ```
#[proc_macro_attribute]
pub fn with_features_try_runtime_and_not_runtime_benchmarks(
	args: proc_macro::TokenStream,
	input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
	let features = [
		RuntimeFeature { name: "sp-runtime/try-runtime".into(), is_enabled: true },
		RuntimeFeature { name: "sp-runtime/runtime-benchmarks".into(), is_enabled: false },
	];

	let mut args = syn::parse_macro_input!(args as ConfigurationPredicate);
	let input = proc_macro2::TokenStream::from(input);

	args.replace_features(&features[..]);

	quote::quote!(
		#[cfg(#args)]
		#input
	)
	.into()
}

// NOTE: This documentation is shown on the re-exported macro in the sp-runtime crate.
// This doc is duplicated for each macro.
/// Extended implementation of `cfg` macro with access to `sp-runtime/try-runtime` and
/// `sp-runtime/runtime-benchmarks` features.
///
/// Syntax is the same as `cfg` macro.
///
/// `#![..]` style macro call are not supported.
///
/// # Example
///
/// ```
/// #[sp_runtime::runtime_cfg(feature = "sp-runtime/try-runtime")]
/// fn some_function() {
/// 	println!("try-runtime is enabled");
/// }
///
/// #[sp_runtime::runtime_cfg(all(feature = "std", feature = "sp-runtime/runtime-benchmarks"))]
/// fn some_function() {
/// 	println!("try-runtime and std are enabled");
/// }
/// ```
#[proc_macro_attribute]
pub fn with_features_not_try_runtime_and_runtime_benchmarks(
	args: proc_macro::TokenStream,
	input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
	let features = [
		RuntimeFeature { name: "sp-runtime/try-runtime".into(), is_enabled: false },
		RuntimeFeature { name: "sp-runtime/runtime-benchmarks".into(), is_enabled: true },
	];

	let mut args = syn::parse_macro_input!(args as ConfigurationPredicate);
	let input = proc_macro2::TokenStream::from(input);

	args.replace_features(&features[..]);

	quote::quote!(
		#[cfg(#args)]
		#input
	)
	.into()
}

// NOTE: This documentation is shown on the re-exported macro in the sp-runtime crate.
// This doc is duplicated for each macro.
/// Extended implementation of `cfg` macro with access to `sp-runtime/try-runtime` and
/// `sp-runtime/runtime-benchmarks` features.
///
/// Syntax is the same as `cfg` macro.
///
/// `#![..]` style macro call are not supported.
///
/// # Example
///
/// ```
/// #[sp_runtime::runtime_cfg(feature = "sp-runtime/try-runtime")]
/// fn some_function() {
/// 	println!("try-runtime is enabled");
/// }
///
/// #[sp_runtime::runtime_cfg(all(feature = "std", feature = "sp-runtime/runtime-benchmarks"))]
/// fn some_function() {
/// 	println!("try-runtime and std are enabled");
/// }
/// ```
#[proc_macro_attribute]
pub fn with_features_not_try_runtime_and_not_runtime_benchmarks(
	args: proc_macro::TokenStream,
	input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
	let features = [
		RuntimeFeature { name: "sp-runtime/try-runtime".into(), is_enabled: false },
		RuntimeFeature { name: "sp-runtime/runtime-benchmarks".into(), is_enabled: false },
	];

	let mut args = syn::parse_macro_input!(args as ConfigurationPredicate);
	let input = proc_macro2::TokenStream::from(input);

	args.replace_features(&features[..]);

	quote::quote!(
		#[cfg(#args)]
		#input
	)
	.into()
}
