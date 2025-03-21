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

//! Helpers for the NoBound derives.

use std::collections::HashSet;
use syn::{DeriveInput, GenericParam, TypeParamBound};

/// Applies a bound (e.g. `::core::clone::Clone`) to all type parameters listed in the
/// #[still_bind(...)] attribute of the input.
///
/// # Parameters
///
/// - `input`: A mutable reference to the `DeriveInput` to modify.
/// - `bound`: A token stream representing the bound to add.
///
/// # Returns
///
/// A `Result` which is `Ok(())` if successful, or a `syn::Error` if parsing fails.
pub fn apply_still_bind(
    input: &mut DeriveInput,
    bound: proc_macro2::TokenStream,
) -> Result<(), syn::Error> {
    // Look for the #[still_bind(...)] attribute and extract its comma-separated identifiers.
    let still_bind_set: Option<HashSet<_>> = input
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("still_bind"))
        .map(|attr| {
            attr.parse_args_with(syn::punctuated::Punctuated::<syn::Ident, syn::Token![,]>::parse_terminated)
                .map(|ids| ids.into_iter().collect::<HashSet<_>>())
        })
        .transpose()?;

    // If the attribute is present, add the provided bound to each matching type parameter.
    if let Some(bind_set) = still_bind_set {
        let parsed_bound: TypeParamBound = syn::parse2(bound)?;
        for param in input.generics.params.iter_mut() {
            if let GenericParam::Type(ref mut type_param) = param {
                if bind_set.contains(&type_param.ident) {
                    type_param.bounds.push(parsed_bound.clone());
                }
            }
        }
    }
    Ok(())
}
