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

/// Derive Clone but do not bound any generic. Optionally select which generics aren't bounded with `no_bounds_for(...)`.
pub fn derive_clone_no_bound(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    // Parse the input tokens into a syntax tree.
    let mut input = syn::parse_macro_input!(input as syn::DeriveInput);

    // Look for a #[no_bounds_for(...)] attribute.
    let no_bounds_set = if let Some(attr) = input.attrs.iter().find(|attr| attr.path().is_ident("no_bounds_for")) {
        match attr.parse_args_with(syn::punctuated::Punctuated::<syn::Ident, syn::Token![,]>::parse_terminated) {
            Ok(ids) => Some(ids.into_iter().collect::<HashSet<_>>()),
            Err(e) => return syn::Error::new(attr.span(), e).to_compile_error().into(),
        }
    } else {
        None
    };

    // If the attribute is present, add a Clone bound to any type parameter not listed.
    if let Some(ref ignore_set) = no_bounds_set {
        for param in input.generics.params.iter_mut() {
            if let syn::GenericParam::Type(ref mut type_param) = param {
                if !ignore_set.contains(&type_param.ident) {
                    type_param.bounds.push(syn::parse_quote!(::core::clone::Clone));
                }
            }
        }
    }

    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let impl_ = match input.data {
        syn::Data::Struct(ref s) => match &s.fields {
            syn::Fields::Named(named) => {
                let fields = named.named.iter().map(|f| {
                    let ident = &f.ident;
                    quote::quote_spanned!(f.span() =>
                        #ident: ::core::clone::Clone::clone(&self.#ident)
                    )
                });
                quote::quote!( Self { #( #fields, )* } )
            },
            syn::Fields::Unnamed(unnamed) => {
                let fields = unnamed.unnamed.iter().enumerate().map(|(i, f)| {
                    let index = syn::Index::from(i);
                    quote::quote_spanned!(f.span() =>
                        ::core::clone::Clone::clone(&self.#index)
                    )
                });
                quote::quote!( Self ( #( #fields, )* ) )
            },
            syn::Fields::Unit => quote::quote!(Self),
        },
        syn::Data::Enum(ref e) => {
            let variants = e.variants.iter().map(|variant| {
                let ident = &variant.ident;
                match &variant.fields {
                    syn::Fields::Named(named) => {
                        let captured = named.named.iter().map(|f| &f.ident);
                        let cloned = captured.clone().map(|ident| {
                            quote::quote_spanned!(ident.span() =>
                                #ident: ::core::clone::Clone::clone(#ident)
                            )
                        });
                        quote::quote!(
                            Self::#ident { #( ref #captured, )* } => Self::#ident { #( #cloned, )* }
                        )
                    },
                    syn::Fields::Unnamed(unnamed) => {
                        let captured = unnamed.unnamed.iter().enumerate().map(|(i, f)| {
                            syn::Ident::new(&format!("_{}", i), f.span())
                        });
                        let cloned = captured.clone().map(|ident| {
                            quote::quote_spanned!(ident.span() =>
                                ::core::clone::Clone::clone(#ident)
                            )
                        });
                        quote::quote!(
                            Self::#ident ( #( ref #captured, )* ) => Self::#ident ( #( #cloned, )* )
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
            return syn::Error::new(input.span(), msg).to_compile_error().into();
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

