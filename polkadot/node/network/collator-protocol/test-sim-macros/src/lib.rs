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
//! Currently exposes a single attribute, [`macro@sim_test`], which is the lightest possible
//! shell around `#[test]`. In Phase C only the legacy validator adapter is registered, so the
//! macro doesn't need to expand to multiple test functions yet. When cross-impl runs land it
//! will fan out to one `#[test]` per registered subsystem adapter — the macro is the seam.

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

/// `#[sim_test]` — marks a function as a simulator-driven test.
///
/// Today this is sugar for `#[test]`. It exists as a future seam: scenario code that uses
/// `#[sim_test]` will fan out to one `#[test]` per subsystem adapter once the framework
/// supports cross-impl runs (Phase D and beyond). Existing scenarios then need no edits.
#[proc_macro_attribute]
pub fn sim_test(attr: TokenStream, item: TokenStream) -> TokenStream {
	if !attr.is_empty() {
		// Reserved for future filters such as `only = "..."` / `skip = "..."`.
		return syn::Error::new_spanned(
			proc_macro2::TokenStream::from(attr),
			"`#[sim_test]` does not yet accept arguments; remove them or upgrade the framework",
		)
		.to_compile_error()
		.into();
	}
	let input = parse_macro_input!(item as ItemFn);
	let expanded = quote! {
		#[::core::prelude::v1::test]
		#input
	};
	expanded.into()
}
