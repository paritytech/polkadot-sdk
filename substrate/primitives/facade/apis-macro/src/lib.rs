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

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use syn::{parse::{Parse, ParseStream}, parse_macro_input, spanned::Spanned, ItemTrait};
use quote::quote;

/// This defines the facade APIs and metadata for them, doing some additional validity checks.
/// This is only expected to be used in the sibling `facade_runtime_apis` crate.
#[proc_macro]
pub fn define_facade_apis(items: TokenStream) -> TokenStream {
    let facade_traits = parse_macro_input!(items as FacadeApiTraits);

    let metadata_fn = generate_metadata_fn(&facade_traits)
        .unwrap_or_else(|e| e.to_compile_error());

    quote! {
        /// Types handed back from [`crate::metadata()`].
        #[cfg(feature = "metadata")]
        pub mod metadata {
            use super::*;
            use alloc::vec::Vec;
            use alloc::vec;
            use scale_info::PortableRegistry;

            /// A type ID.
            pub type TypeId = u32;

            pub struct FacadeMetadata {
                /// Type registry to contain all referenced types.
                pub types: PortableRegistry,
                /// List of facade APIs.
                pub apis: Vec<FacadeApi>,
            }

            pub struct FacadeApi {
                /// Trait name.
                pub name: &'static str,
                /// Methods on the trait. 
                pub methods: Vec<FacadeApiMethod>,
                /// Trait docs.
                pub docs: &'static str
            }

            pub struct FacadeApiMethod {
                /// Method name.
                pub name: &'static str,
                /// What version did this method become available.
                pub version: usize,
                /// Method parameters.
                pub inputs: Vec<FacadeApiMethodParam>,
                /// Method output.
                pub output: TypeId,
                /// Method documentation.
                pub docs: &'static str,
            }

            pub struct FacadeApiMethodParam {
                /// Parameter type.
                pub ty: TypeId,
            }

            #metadata_fn
        }

        #[cfg(feature = "metadata")]
        pub use metadata::metadata;

        #[cfg(feature = "decl-runtime-apis")]
        sp_api::decl_runtime_apis! {
            #facade_traits
        }
    }.into()
}

const API_VERSION_ATTR: &str = "api_version";
const CHANGED_IN_ATTR: &str = "changed_in";
const DOC_ATTR: &str = "doc";

/// A small wrapper to allow parsing tokens to/from a vec of trait definitions.
struct FacadeApiTraits {
	decls: Vec<ItemTrait>,
}

impl Parse for FacadeApiTraits {
	fn parse(input: ParseStream) -> Result<Self, syn::Error> {
		let mut decls = Vec::new();

		while !input.is_empty() {
            let item_trait = ItemTrait::parse(input)
                .map_err(|e| {
                    syn::Error::new(e.span(), "Only trait definitions are allowed in the define_facade_apis! macro. Define other things outside of it.")
                })?;
            validate_trait(&item_trait)?;
			decls.push(item_trait);
		}

		Ok(Self { decls })
	}
}

impl quote::ToTokens for FacadeApiTraits {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        for item in &self.decls {
            item.to_tokens(tokens);
        }
    }
}

/// Check that a trait passed to our `define_facade_apis` macro adheres
/// to the things we want. We:
/// 
/// - Check that traits don't specify `#[api_version(..)]`; we always should support V1 onwards.
/// - Traits should be prefixed with `Facade` to prevent overlap.
/// - Disallow `#[changed_in(..)]` since it breaks backward compatibility.
/// - Disallow anything other than methods in trait definitions.
/// - Ensure that docs exist for all facade APIs.
fn validate_trait(item_trait: &ItemTrait) -> Result<(), syn::Error> {
    if !has_docs(&item_trait.attrs) {
        return Err(syn::Error::new(item_trait.span(), "Facade API traits must all be documented"));
    }

    for attr in &item_trait.attrs {
        if attr.path().is_ident(API_VERSION_ATTR.into()) {
            // Dev note: Runtime APIs have a version which we can use to determine the methods available.
            // New methods can be added with an `#[api_version(2)]` type attr to denote that they only exist from
            // that version onwards. We shouldn't set the `api_version` of the entire trait though, as we expect to
            // always support the V1 methods to avoid breaking compatibility.
            return Err(syn::Error::new(attr.path().span(), "The 'api_version' attribute should not be used on the trait definition: the Facade traits should contain all methods from v1 up to avoid breaking compatibility."))
        }
        if !attr.path().is_ident(DOC_ATTR.into()) {
            // This is just a safety mechanism to ensure that if new attrs are added to `decl_runtime_apis`, we must 
            // manually "whitelist" them in this crate to ensure that they are properly taken into account.
            return Err(syn::Error::new(attr.path().span(), "Only doc attributes are allowed on the trait definition."))
        }
    }

    if !item_trait.ident.to_string().starts_with("Facade") {
        let err = format!("All facade trait names must start with `Facade` to disambiguate them from other traits. Consider renaming this to 'Facade{}'", item_trait.ident.to_string());
        return Err(syn::Error::new(item_trait.ident.span(), err))
    }

    for item in &item_trait.items {
        let trait_item_fn = match item {
            syn::TraitItem::Fn(trait_item_fn) => {
                trait_item_fn
            },
            // Only trait functions are expected. Anything else is currently an error.
            _ => {
                return Err(syn::Error::new(item.span(), "Only functions are supported in traits."))
            },
        };

        if !has_docs(&trait_item_fn.attrs) {
            return Err(syn::Error::new(trait_item_fn.span(), "Facade API methods must all be documented"));
        }

        for attr in &trait_item_fn.attrs {
            // We prevent `#[changed_in(..)]` because it changes the syntax used to call that method, and we want to avoid breaking changes.
            if attr.path().is_ident(CHANGED_IN_ATTR.into()) {
                return Err(syn::Error::new(attr.path().span(), "To avoid breaking our stability guarantees, `#[changed_in(..)]` is not supported. Define a new method with `#[api_version(..)] instead."))
            }
            // On a method, we can add `#[api_version(..)]` to denote that the method is only available from that version onwards.
            // The original methods should never be touched to try to preserve stability going forwards.
            if !attr.path().is_ident(DOC_ATTR.into()) && !attr.path().is_ident(API_VERSION_ATTR.into()) {
                return Err(syn::Error::new(attr.path().span(), "Only the `#[api_version(..)]` attribute is allowed on trait methods to denote which version they are available from."))
            }
        }
    }

    // Check that any api_versions listed are sequential, ie we shouldn't see only
    // #[api_version(3)], because where is the definition that version 2 added in that case?
    // In other words: if we have a version 4 method, we expect to see a version 1, 2 and 3 method too somewhere.
    {
        use std::collections::HashMap;
        let versions = item_trait.items
            .iter()
            .filter_map(|item| {
                match item {
                    syn::TraitItem::Fn(f) => Some(f),
                    _ => None
                }
            })
            .map(|f| get_api_version(&f.attrs).map(|n| (n, f.sig.ident.span())))
            .collect::<Result<HashMap<usize, Span>, syn::Error>>()?;

        for version in 1 ..= versions.len() {
            if !versions.contains_key(&version) {
                let err_msg = format!("api_versions should be sequential, but version {version} was not found.");
                return Err(syn::Error::new(item_trait.ident.span(), err_msg))
            }
        }
    }

    Ok(())
}

/// Check that some set of attributes contains at least one doc attr.
fn has_docs(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a: &syn::Attribute| a.path().is_ident("doc".into()))
}

/// generate a function which constructs and returns facade metadata given the facade traits we've defined.
/// This metadata can then be used to eg compare the definitions with runtime APIs in RuntimeMetadata
/// for instance.
fn generate_metadata_fn(facade_traits: &FacadeApiTraits) -> Result<TokenStream2, syn::Error> {
    let apis = facade_traits.decls.iter().map(|t| {
        let trait_name = t.ident.to_string();
        let docs = get_docs(&t.attrs);

        let methods = t.items.iter().map(|item| {
            let syn::TraitItem::Fn(method) = item else { return TokenStream2::new() };

            let method_name = method.sig.ident.to_string();
            let method_docs = get_docs(&method.attrs);
            let method_version = get_api_version(&method.attrs)
                .map(|n| quote! { #n })
                .unwrap_or_else(|e| e.to_compile_error());
            let method_output_ty = match &method.sig.output {
                syn::ReturnType::Default => quote!{ () },
                syn::ReturnType::Type(_, t) => quote!{ #t },
            };
            
            let method_params = method.sig.inputs.iter().map(|input| {
                let syn::FnArg::Typed(input) = input else {
                    return syn::Error::new(input.span(), "self types not supported here.").to_compile_error()
                };
                let param_ty = &input.ty;

                quote! {
                    FacadeApiMethodParam {
                        ty: {
                            let m = scale_info::MetaType::new::<#param_ty>();
                            type_registry.register_type(&m).id
                        }
                    }
                }
            });

            quote! {
                FacadeApiMethod {
                    name: #method_name,
                    version: #method_version,
                    inputs: vec![ #(#method_params),* ],
                    output: {
                        let m = scale_info::MetaType::new::<#method_output_ty>();
                        type_registry.register_type(&m).id
                    },
                    docs: #method_docs,
                }
            }
        });

        quote! {
            FacadeApi {
                name: #trait_name,
                methods: vec![ #(#methods),* ],
                docs: #docs,
            }
        }
    });

    let output = quote! {
        /// Construct and return metadata about the facade APIs.
        pub fn metadata() -> FacadeMetadata {           
            // Start with empty type registry:
            let mut type_registry = scale_info::Registry::new();

            // The code injected here will push types to the above:
            let apis = vec![ #(#apis),* ];

            let types = type_registry.into();
            FacadeMetadata { apis, types }
        }
    };

    Ok(output)
}

/// Extract the doc string from some set of attributes and return it as a string literal token.
fn get_docs(attrs: &[syn::Attribute]) -> TokenStream2 {
    let mut docs = String::new();
    let mut is_first = true;
    for attr in attrs {
        if attr.path().is_ident(DOC_ATTR) {
            let syn::Meta::NameValue(nv) = &attr.meta else {
                return syn::Error::new(attr.meta.span(), "Doc string expected to take form #[doc = \"value\")").to_compile_error()
            };
            let syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(s), .. }) = &nv.value else {
                return syn::Error::new(attr.meta.span(), "Doc string is expected to be a string literal").to_compile_error()
            };

            if !is_first {
                docs.push('\n'); 
            }
            is_first = false;

            docs.push_str(&s.value().trim());
        }
    }

    quote! { #docs }
}

/// Extract the `#[api_version(N)]` number from some set of attributes and return it.
fn get_api_version(attrs: &[syn::Attribute]) -> Result<usize, syn::Error> {
    let Some(api_version_attr) = attrs
        .iter()
        .find(|a| a.path().is_ident(API_VERSION_ATTR)) else { return Ok(1) };

    let Ok(version) = api_version_attr.parse_args::<syn::LitInt>() else {
        return Err(syn::Error::new(api_version_attr.meta.span(), "Cannot parse api version"))
    };

    Ok(version.base10_parse::<usize>()?)
}