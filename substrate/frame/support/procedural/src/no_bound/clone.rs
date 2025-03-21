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

use syn::spanned::Spanned;
use std::collections::HashSet;

/// Derive Clone but do not bound any generic. Optionally select which generics are still bounded with `still_bind(...)`.
pub fn derive_clone_no_bound(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let mut input = syn::parse_macro_input!(input as syn::DeriveInput);

    // Look for a #[still_bind(...)] attribute.
    let still_bind_set = if let Some(attr) = input.attrs.iter().find(|attr| attr.path().is_ident("still_bind")) {
        match attr.parse_args_with(syn::punctuated::Punctuated::<syn::Ident, syn::Token![,]>::parse_terminated) {
            Ok(ids) => Some(ids.into_iter().collect::<HashSet<_>>()),
            Err(e) => return syn::Error::new(attr.span(), e).to_compile_error().into(),
        }
    } else {
        None
    };

    // If the attribute is present, add a Clone bound to any type parameter listed.
    if let Some(ref bind_set) = still_bind_set {
        for param in input.generics.params.iter_mut() {
            if let syn::GenericParam::Type(ref mut type_param) = param {
                if bind_set.contains(&type_param.ident) {
                    type_param.bounds.push(syn::parse_quote!(::core::clone::Clone));
                }
            }
        }
    }

    let name = &input.ident;
	let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

	let impl_ = match input.data {
		syn::Data::Struct(struct_) => match struct_.fields {
			syn::Fields::Named(named) => {
				let fields = named.named.iter().map(|i| &i.ident).map(|i| {
					quote::quote_spanned!(i.span() =>
						#i: ::core::clone::Clone::clone(&self.#i)
					)
				});

				quote::quote!( Self { #( #fields, )* } )
			},
			syn::Fields::Unnamed(unnamed) => {
				let fields =
					unnamed.unnamed.iter().enumerate().map(|(i, _)| syn::Index::from(i)).map(|i| {
						quote::quote_spanned!(i.span() =>
							::core::clone::Clone::clone(&self.#i)
						)
					});

				quote::quote!( Self ( #( #fields, )* ) )
			},
			syn::Fields::Unit => {
				quote::quote!(Self)
			},
		},
		syn::Data::Enum(enum_) => {
			let variants = enum_.variants.iter().map(|variant| {
				let ident = &variant.ident;
				match &variant.fields {
					syn::Fields::Named(named) => {
						let captured = named.named.iter().map(|i| &i.ident);
						let cloned = captured.clone().map(|i| {
							::quote::quote_spanned!(i.span() =>
								#i: ::core::clone::Clone::clone(#i)
							)
						});
						quote::quote!(
							Self::#ident { #( ref #captured, )* } => Self::#ident { #( #cloned, )*}
						)
					},
					syn::Fields::Unnamed(unnamed) => {
						let captured = unnamed
							.unnamed
							.iter()
							.enumerate()
							.map(|(i, f)| syn::Ident::new(&format!("_{}", i), f.span()));
						let cloned = captured.clone().map(|i| {
							quote::quote_spanned!(i.span() =>
								::core::clone::Clone::clone(#i)
							)
						});
						quote::quote!(
							Self::#ident ( #( ref #captured, )* ) => Self::#ident ( #( #cloned, )*)
						)
					},
					syn::Fields::Unit => quote::quote!( Self::#ident => Self::#ident ),
				}
			});

			quote::quote!(match self {
				#( #variants, )*
			})
		},
		syn::Data::Union(_) => {
			let msg = "Union type not supported by `derive(CloneNoBound)`";
			return syn::Error::new(input.span(), msg).to_compile_error().into()
		},
	};

	quote::quote!(
		const _: () = {
			#[automatically_derived]
			impl #impl_generics ::core::clone::Clone for #name #ty_generics #where_clause {
				fn clone(&self) -> Self {
					#impl_
				}
			}
		};
	)
	.into()
}

