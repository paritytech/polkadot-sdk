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

//! Proc-macros for the collator-protocol deterministic test framework.
//!
//! [`macro@sim_test`] marks a generic scenario function and fans it out to one `#[test]`
//! shell per registered subsystem-under-test implementation. The result is that every
//! scenario doubles as a differential test: the same prose runs against `LegacyValidator`
//! and `ExperimentalValidator`, and any divergence in observable behaviour fails the test.
//!
//! Registration of implementations is hardcoded into this macro for now (just the two
//! validator-side variants). When more impls onboard, the registry can become an attribute
//! argument or a separate `register_impls!` macro.
//!
//! # Usage
//!
//! ```ignore
//! #[sim_test]
//! fn my_scenario<S>()
//! where
//!     S: polkadot_collator_protocol_test_sim::harness::SubsystemUnderTest<
//!         Message = polkadot_node_subsystem::messages::CollatorProtocolMessage,
//!     >,
//!     polkadot_node_subsystem::messages::AllMessages: From<
//!         <S::Message as polkadot_overseer::AssociateOutgoing>::OutgoingMessages,
//!     >,
//!     polkadot_node_subsystem::messages::AllMessages: From<S::Message>,
//! {
//!     // ...scenario body parameterised over S...
//! }
//! ```
//!
//! Expands to two `#[test]` functions, `my_scenario__legacy` and `my_scenario__experimental`,
//! each calling `my_scenario::<LegacyValidator>()` / `my_scenario::<ExperimentalValidator>()`.

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse::Parser, parse_macro_input, ItemFn, LitStr, Token};

/// `#[sim_test]` — fan-out attribute for differential scenarios.
///
/// The annotated function must be generic over a single type parameter conventionally
/// named `S` and bounded as a [`SubsystemUnderTest`]. The macro generates one
/// `#[test]`-marked wrapper per registered implementation; each wrapper instantiates the
/// generic body with the corresponding adapter type and calls it.
///
/// # Filters
///
/// `#[sim_test(only = "legacy")]` runs the scenario against `LegacyValidator` only.
/// `#[sim_test(only = "experimental")]` runs against `ExperimentalValidator` only.
/// `#[sim_test(skip = "legacy")]` / `#[sim_test(skip = "experimental")]` invert the filter.
/// Unfiltered (`#[sim_test]`) runs against both.
///
/// Use filters when an implementation deliberately differs in observable behaviour and the
/// scenario captures the legacy-only or experimental-only contract. Most scenarios should be
/// unfiltered: any divergence is a bug worth knowing about.
///
/// [`SubsystemUnderTest`]: ../polkadot_collator_protocol_test_sim/harness/sim/trait.SubsystemUnderTest.html
#[proc_macro_attribute]
pub fn sim_test(attr: TokenStream, item: TokenStream) -> TokenStream {
	let filter = match parse_filter(attr) {
		Ok(f) => f,
		Err(e) => return e.to_compile_error().into(),
	};
	let input = parse_macro_input!(item as ItemFn);
	let fn_name = input.sig.ident.clone();
	let legacy_name = format_ident!("{}__legacy", fn_name);
	let experimental_name = format_ident!("{}__experimental", fn_name);

	let legacy_test = if filter.includes_legacy() {
		quote! {
			#[::core::prelude::v1::test]
			#[allow(non_snake_case)]
			fn #legacy_name() {
				#fn_name::<crate::impls::LegacyValidator>();
			}
		}
	} else {
		quote! {}
	};
	let experimental_test = if filter.includes_experimental() {
		quote! {
			#[::core::prelude::v1::test]
			#[allow(non_snake_case)]
			fn #experimental_name() {
				#fn_name::<crate::impls::ExperimentalValidator>();
			}
		}
	} else {
		quote! {}
	};

	let expanded = quote! {
		#input
		#legacy_test
		#experimental_test
	};
	expanded.into()
}

#[derive(Clone, Copy)]
enum Filter {
	Both,
	OnlyLegacy,
	OnlyExperimental,
}

impl Filter {
	fn includes_legacy(self) -> bool {
		matches!(self, Filter::Both | Filter::OnlyLegacy)
	}

	fn includes_experimental(self) -> bool {
		matches!(self, Filter::Both | Filter::OnlyExperimental)
	}
}

fn parse_filter(attr: TokenStream) -> syn::Result<Filter> {
	if attr.is_empty() {
		return Ok(Filter::Both);
	}
	let parser = |input: syn::parse::ParseStream| -> syn::Result<Filter> {
		let key: syn::Ident = input.parse()?;
		input.parse::<Token![=]>()?;
		let value: LitStr = input.parse()?;
		let value = value.value();
		match (key.to_string().as_str(), value.as_str()) {
			("only", "legacy") => Ok(Filter::OnlyLegacy),
			("only", "experimental") => Ok(Filter::OnlyExperimental),
			("skip", "legacy") => Ok(Filter::OnlyExperimental),
			("skip", "experimental") => Ok(Filter::OnlyLegacy),
			_ => Err(syn::Error::new(
				key.span(),
				"`#[sim_test]` filters: only/skip = \"legacy\" | \"experimental\"",
			)),
		}
	};
	parser.parse(attr)
}
