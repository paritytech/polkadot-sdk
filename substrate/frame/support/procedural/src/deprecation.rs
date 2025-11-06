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

use proc_macro2::TokenStream;
use quote::quote;
use syn::{
	punctuated::Punctuated, spanned::Spanned, Error, Expr, ExprLit, Lit, Meta, MetaNameValue,
	Result, Token, Variant,
};

fn deprecation_msg_formatter(msg: &str) -> String {
	format!(
		r#"{msg}
		help: the following are the possible correct uses
|
|     #[deprecated = "reason"]
|
|     #[deprecated(/*opt*/ since = "version", /*opt*/ note = "reason")]
|
|     #[deprecated]
|"#
	)
}

// Should we generate a #crate_::__private::metadata_ir::ItemDeprecationInfoIR
// or a #crate_::__private::metadata_ir::VariantDeprecationInfoIR? In other words,
// are we targeting variant deprecation information, or generic item deprecation
// information.
#[derive(Copy, Clone, PartialEq)]
enum DeprecationTarget {
	Item,
	Variant,
}

fn parse_deprecated_meta(
	crate_: &TokenStream,
	attr: &syn::Attribute,
	target: DeprecationTarget,
) -> Result<TokenStream> {
	let target = match target {
		DeprecationTarget::Item =>
			quote! { #crate_::__private::metadata_ir::ItemDeprecationInfoIR },
		DeprecationTarget::Variant =>
			quote! { #crate_::__private::metadata_ir::VariantDeprecationInfoIR },
	};

	match &attr.meta {
		Meta::List(meta_list) => {
			let parsed = meta_list
				.parse_args_with(Punctuated::<MetaNameValue, Token![,]>::parse_terminated)
				.map_err(|e| Error::new(attr.span(), e.to_string()))?;
			let (note, since) = parsed.iter().try_fold((None, None), |mut acc, item| {
				let value = match &item.value {
					Expr::Lit(ExprLit { lit: lit @ Lit::Str(_), .. }) => Ok(lit),
					_ => Err(Error::new(
						attr.span(),
						deprecation_msg_formatter(
							"Invalid deprecation attribute: expected string literal",
						),
					)),
				}?;
				if item.path.is_ident("note") {
					acc.0.replace(value);
				} else if item.path.is_ident("since") {
					acc.1.replace(value);
				}
				Ok::<(Option<&syn::Lit>, Option<&syn::Lit>), Error>(acc)
			})?;
			note.map_or_else(
				|| {
					Err(Error::new(
						attr.span(),
						deprecation_msg_formatter("Invalid deprecation attribute: missing `note`"),
					))
				},
				|note| {
					let since = if let Some(str) = since {
						quote! { Some(#str) }
					} else {
						quote! { None }
					};
					let doc = quote! { #target::Deprecated { note: #note, since: #since }};
					Ok(doc)
				},
			)
		},
		Meta::NameValue(MetaNameValue {
			value: Expr::Lit(ExprLit { lit: lit @ Lit::Str(_), .. }),
			..
		}) => {
			// #[deprecated = "lit"]
			let doc = quote! { #target::Deprecated { note: #lit, since: None } };
			Ok(doc)
		},
		Meta::Path(_) => {
			// #[deprecated]
			Ok(quote! { #target::DeprecatedWithoutNote })
		},
		_ => Err(Error::new(
			attr.span(),
			deprecation_msg_formatter("Invalid deprecation attribute: expected string literal"),
		)),
	}
}

fn find_deprecation_attr(attrs: &[syn::Attribute]) -> Option<&syn::Attribute> {
	attrs.iter().find(|a| a.path().is_ident("deprecated"))
}

fn parse_deprecation(
	path: &TokenStream,
	attrs: &[syn::Attribute],
	target: DeprecationTarget,
) -> Result<Option<TokenStream>> {
	find_deprecation_attr(attrs)
		.map(|a| parse_deprecated_meta(path, a, target))
		.transpose()
}

/// collects deprecation attribute if its present.
pub fn get_deprecation(path: &TokenStream, attrs: &[syn::Attribute]) -> Result<TokenStream> {
	parse_deprecation(path, attrs, DeprecationTarget::Item).map(|item| {
		item.unwrap_or_else(|| {
			quote! {#path::__private::metadata_ir::ItemDeprecationInfoIR::NotDeprecated}
		})
	})
}

/// Call this on enum attributes to return an error if the #[deprecation] attribute is found.
pub fn prevent_deprecation_attr_on_outer_enum(parent_attrs: &[syn::Attribute]) -> Result<()> {
	if let Some(attr) = find_deprecation_attr(parent_attrs) {
		return Err(Error::new(
			attr.span(),
			"The `#[deprecated]` attribute should be applied to individual variants, not the enum as a whole.",
		));
	}
	Ok(())
}

/// collects deprecation attribute if its present for enum-like types
pub fn get_deprecation_enum<'a>(
	path: &TokenStream,
	children_attrs: impl Iterator<Item = (u8, &'a [syn::Attribute])>,
) -> Result<TokenStream> {
	let children = children_attrs
		.filter_map(|(key, attributes)| {
			let deprecation_status =
				parse_deprecation(path, attributes, DeprecationTarget::Variant).transpose();
			deprecation_status.map(|item| item.map(|item| quote::quote! { (#key, #item) }))
		})
		.collect::<Result<Vec<TokenStream>>>()?;

	if children.is_empty() {
		Ok(
			quote::quote! { #path::__private::metadata_ir::EnumDeprecationInfoIR::nothing_deprecated() },
		)
	} else {
		let children = quote::quote! { #path::__private::scale_info::prelude::collections::BTreeMap::from([#( #children),*]) };
		Ok(quote::quote! { #path::__private::metadata_ir::EnumDeprecationInfoIR(#children) })
	}
}

/// Gets the index for the variant inside `Error` or `Event` declaration.
/// priority is as follows:
/// Manual `#[codec(index = N)]`
/// Explicit discriminant `Variant = N`
/// Variant's definition index
pub fn variant_index_for_deprecation(index: u8, item: &Variant) -> u8 {
	let index: u8 =
		if let Some((_, Expr::Lit(ExprLit { lit: Lit::Int(num_lit), .. }))) = &item.discriminant {
			num_lit.base10_parse::<u8>().unwrap_or(index as u8)
		} else {
			index as u8
		};

	item.attrs
		.iter()
		.find(|attr| attr.path().is_ident("codec"))
		.and_then(|attr| {
			if let Meta::List(meta_list) = &attr.meta {
				meta_list
					.parse_args_with(Punctuated::<MetaNameValue, syn::Token![,]>::parse_terminated)
					.ok()
			} else {
				None
			}
		})
		.and_then(|parsed| {
			parsed.iter().fold(None, |mut acc, item| {
				if let Expr::Lit(ExprLit { lit: Lit::Int(num_lit), .. }) = &item.value {
					num_lit.base10_parse::<u8>().iter().for_each(|val| {
						if item.path.is_ident("index") {
							acc.replace(*val);
						}
					})
				};
				acc
			})
		})
		.unwrap_or(index)
}

/// Filters all of the `allow` and `deprecated` attributes.
///
/// `allow` attributes are returned as is while `deprecated` attributes are replaced by
/// `#[allow(deprecated)]`.
pub fn extract_or_return_allow_attrs(
	items: &[syn::Attribute],
) -> impl Iterator<Item = syn::Attribute> + '_ {
	items.iter().filter_map(|attr| {
		attr.path().is_ident("allow").then(|| attr.clone()).or_else(|| {
			attr.path().is_ident("deprecated").then(|| {
				syn::parse_quote! {
					#[allow(deprecated)]
				}
			})
		})
	})
}
