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

use crate::pallet::Def;
use proc_macro2::TokenStream;

/// Expands `composite_enum` and adds the `VariantCount` implementation for it."
pub fn expand_composites(def: &mut Def) -> TokenStream {
	let mut expand = quote::quote!();
	let frame_support = &def.frame_support;

	for composite in &def.composites {
		let name = &composite.ident;
		let (impl_generics, ty_generics, where_clause) = composite.generics.split_for_impl();
		let variants_count = composite.variant_count;

		// add `VariantCount` implementation for `composite_enum`
		expand.extend(quote::quote_spanned!(composite.attr_span =>
			impl #impl_generics #frame_support::traits::VariantCount for #name #ty_generics #where_clause {
				const VARIANT_COUNT: u32 = #variants_count;
			}
		));
	}

	expand
}
