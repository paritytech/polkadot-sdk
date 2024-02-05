// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Procedural macros used in XCM.

use proc_macro::TokenStream;
use syn::{parse_macro_input, DeriveInput};

mod builder_pattern;
mod v2;
mod v3;
mod v4;
mod weight_info;

#[proc_macro]
pub fn impl_conversion_functions_for_multilocation_v2(input: TokenStream) -> TokenStream {
	v2::multilocation::generate_conversion_functions(input)
		.unwrap_or_else(syn::Error::into_compile_error)
		.into()
}

#[proc_macro]
pub fn impl_conversion_functions_for_junctions_v2(input: TokenStream) -> TokenStream {
	v2::junctions::generate_conversion_functions(input)
		.unwrap_or_else(syn::Error::into_compile_error)
		.into()
}

#[proc_macro_derive(XcmWeightInfoTrait)]
pub fn derive_xcm_weight_info(item: TokenStream) -> TokenStream {
	weight_info::derive(item)
}

#[proc_macro]
pub fn impl_conversion_functions_for_multilocation_v3(input: TokenStream) -> TokenStream {
	v3::multilocation::generate_conversion_functions(input)
		.unwrap_or_else(syn::Error::into_compile_error)
		.into()
}

#[proc_macro]
pub fn impl_conversion_functions_for_junctions_v3(input: TokenStream) -> TokenStream {
	v3::junctions::generate_conversion_functions(input)
		.unwrap_or_else(syn::Error::into_compile_error)
		.into()
}

#[proc_macro]
pub fn impl_conversion_functions_for_location_v4(input: TokenStream) -> TokenStream {
	v4::location::generate_conversion_functions(input)
		.unwrap_or_else(syn::Error::into_compile_error)
		.into()
}

#[proc_macro]
pub fn impl_conversion_functions_for_junctions_v4(input: TokenStream) -> TokenStream {
	v4::junctions::generate_conversion_functions(input)
		.unwrap_or_else(syn::Error::into_compile_error)
		.into()
}

/// This is called on the `Instruction` enum, not on the `Xcm` struct,
/// and allows for the following syntax for building XCMs:
/// let message = Xcm::builder()
/// 	.withdraw_asset(assets)
/// 	.buy_execution(fees, weight_limit)
/// 	.deposit_asset(assets, beneficiary)
/// 	.build();
#[proc_macro_derive(Builder, attributes(builder))]
pub fn derive_builder(input: TokenStream) -> TokenStream {
	let input = parse_macro_input!(input as DeriveInput);
	builder_pattern::derive(input)
		.unwrap_or_else(syn::Error::into_compile_error)
		.into()
}
