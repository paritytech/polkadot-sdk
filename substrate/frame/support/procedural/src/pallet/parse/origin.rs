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

use super::helper;
use quote::ToTokens;
use syn::spanned::Spanned;

mod keyword {
	syn::custom_keyword!(authorized_call);
	syn::custom_keyword!(pallet);
}

/// Definition of the pallet origin type.
///
/// Either:
/// * `type Origin`
/// * `struct Origin`
/// * `enum Origin`
pub struct OriginDef {
	pub is_generic: bool,
	/// A set of usage of instance, must be check for consistency with trait.
	pub instances: Vec<helper::InstanceUsage>,
	/// The variant for the authorized call.
	pub authorized_call: Option<(usize, proc_macro2::Span)>,
	/// The index of origin item in pallet module.
	pub index: usize,
	/// The span pointing to the attribute pallet::origin.
	pub span: proc_macro2::Span,
}

/// Possible attributes on enum variants. Right now only pallet::authorized_call.
pub struct EnumVariantAttr {
	span: proc_macro2::Span,
}

impl syn::parse::Parse for EnumVariantAttr {
	fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
		input.parse::<syn::Token![#]>()?;
		let content;
		syn::bracketed!(content in input);
		content.parse::<keyword::pallet>()?;
		content.parse::<syn::Token![::]>()?;
		let span = content.span();
		content.parse::<keyword::authorized_call>()?;
		Ok(EnumVariantAttr { span })
	}
}

impl OriginDef {
	pub fn try_from(
		span: proc_macro2::Span,
		index: usize,
		item: &mut syn::Item,
	) -> syn::Result<Self> {
		let item_span = item.span();
		let (vis, ident, generics, authorized_call) = match item {
			syn::Item::Enum(item) => {
				let mut authorized_call = None;

				for (index, variant) in item.variants.iter_mut().enumerate() {
					let mut attrs: Vec<EnumVariantAttr> = helper::take_item_pallet_attrs(variant)?;

					if attrs.len() > 1 {
						let msg = "Invalid pallet::origin, expected at most one \
							pallet::authorized_call attribute";
						return Err(syn::Error::new(attrs[1].span, msg))
					}

					if let Some(attr) = attrs.pop() {
						if authorized_call.is_some() {
							let msg = "Invalid pallet::origin, expected at most one variant with \
								pallet::authorized_call attribute";
							return Err(syn::Error::new(variant.span(), msg))
						}

						if variant.ident != "AuthorizedCall" {
							let msg = "Invalid pallet::authorized_call, expected variant ident to \
								be `AuthorizedCall`";
							return Err(syn::Error::new(variant.ident.span(), msg))
						}

						let syn::Fields::Unnamed(fields) = &variant.fields else {
							let msg = "Invalid pallet::authorized_call, expected variant fields \
								to be `(_)`";
							return Err(syn::Error::new(variant.fields.span(), msg))
						};

						if syn::parse2::<syn::Token![_]>(fields.unnamed.to_token_stream()).is_err()
						{
							let msg = "Invalid pallet::authorized_call, expected variant fields \
								to be `_`";
							return Err(syn::Error::new(fields.unnamed.span(), msg))
						}

						authorized_call = Some((index, attr.span));
					}
				}

				(&item.vis, &item.ident, &item.generics, authorized_call)
			},
			syn::Item::Struct(item) => (&item.vis, &item.ident, &item.generics, None),
			syn::Item::Type(item) => (&item.vis, &item.ident, &item.generics, None),
			_ => {
				let msg = "Invalid pallet::origin, expected enum or struct or type";
				return Err(syn::Error::new(item.span(), msg))
			},
		};

		let is_generic = !generics.params.is_empty();

		let mut instances = vec![];
		if let Some(u) = helper::check_type_def_optional_gen(generics, item_span)? {
			instances.push(u);
		} else {
			// construct_runtime only allow generic event for instantiable pallet.
			instances.push(helper::InstanceUsage { has_instance: false, span: ident.span() })
		}

		if !matches!(vis, syn::Visibility::Public(_)) {
			let msg = "Invalid pallet::origin, Origin must be public";
			return Err(syn::Error::new(item_span, msg))
		}

		if ident != "Origin" {
			let msg = "Invalid pallet::origin, ident must `Origin`";
			return Err(syn::Error::new(ident.span(), msg))
		}

		Ok(OriginDef { is_generic, instances, authorized_call, index, span })
	}
}
