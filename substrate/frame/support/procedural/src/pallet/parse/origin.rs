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
use syn::spanned::Spanned;

/// Per-variant account-like definition parsed from `#[pallet::as_account(...)]`
/// and optional `#[pallet::nonce_provider]` / `#[pallet::fee_payer]` flags.
pub struct OriginAccountLikeDef {
	/// The variant identifier.
	pub variant_ident: syn::Ident,
	/// The fields of the variant (for generating destructure patterns).
	pub fields: syn::Fields,
	/// The `#[cfg(...)]` attributes of the variant, propagated to the code generated for it.
	pub cfg_attrs: Vec<syn::Attribute>,
	/// The user's closure/function expression for `as_account`.
	pub expr: syn::Expr,
	/// Whether this variant also has `#[pallet::nonce_provider]`.
	pub is_nonce_provider: bool,
	/// Whether this variant also has `#[pallet::fee_payer]`.
	pub is_fee_payer: bool,
}

/// Check a `#[pallet::nonce_provider]` / `#[pallet::fee_payer]` flag attribute on a variant.
///
/// These are plain flags which reuse the closure of `#[pallet::as_account(...)]`, so they must
/// appear exactly as `#[pallet::<name>]`, at most once per variant.
fn check_flag_attr(attr: &syn::Attribute, name: &str, already_present: bool) -> Option<syn::Error> {
	if !matches!(attr.meta, syn::Meta::Path(_)) {
		return Some(syn::Error::new(
			attr.span(),
			format!(
				"`#[pallet::{name}]` does not take any arguments, it reuses the closure given to \
				`#[pallet::as_account(...)]`",
			),
		));
	}
	if already_present {
		return Some(syn::Error::new(
			attr.span(),
			format!("Duplicate `#[pallet::{name}]` attribute on variant"),
		));
	}
	None
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
	/// Per-variant account-like defs. Only populated for enum origins.
	pub account_like_defs: Vec<OriginAccountLikeDef>,
}

impl OriginDef {
	pub fn try_from(item: &mut syn::Item) -> syn::Result<Self> {
		let item_span = item.span();
		let (vis, ident, generics) = match &item {
			syn::Item::Enum(item) => (&item.vis, &item.ident, &item.generics),
			syn::Item::Struct(item) => (&item.vis, &item.ident, &item.generics),
			syn::Item::Type(item) => (&item.vis, &item.ident, &item.generics),
			_ => {
				let msg = "Invalid pallet::origin, expected enum or struct or type";
				return Err(syn::Error::new(item.span(), msg));
			},
		};

		let is_generic = !generics.params.is_empty();

		let mut instances = vec![];
		if let Some(u) = helper::check_type_def_optional_gen(generics, item.span())? {
			instances.push(u);
		} else {
			// construct_runtime only allow generic event for instantiable pallet.
			instances.push(helper::InstanceUsage { has_instance: false, span: ident.span() })
		}

		if !matches!(vis, syn::Visibility::Public(_)) {
			let msg = "Invalid pallet::origin, Origin must be public";
			return Err(syn::Error::new(item_span, msg));
		}

		if ident != "Origin" {
			let msg = "Invalid pallet::origin, ident must `Origin`";
			return Err(syn::Error::new(ident.span(), msg));
		}

		let mut account_like_defs = vec![];

		// Parse #[pallet::as_account(...)], #[pallet::nonce_provider], #[pallet::fee_payer]
		// on enum variants. Only enum types are supported.
		if let syn::Item::Enum(item_enum) = item {
			for variant in item_enum.variants.iter_mut() {
				let mut as_account_attr = None;
				let mut as_account_count = 0;
				let mut has_nonce_provider = false;
				let mut has_fee_payer = false;
				let mut nonce_provider_span = None;
				let mut fee_payer_span = None;
				// First misuse of a flag attribute: a duplicate, or one given arguments.
				let mut flag_error: Option<syn::Error> = None;

				// Find and extract the pallet attributes
				variant.attrs.retain(|attr| {
					if attr.path().segments.len() == 2 && attr.path().segments[0].ident == "pallet"
					{
						let attr_name = attr.path().segments[1].ident.to_string();
						match attr_name.as_str() {
							"as_account" => {
								as_account_count += 1;
								if as_account_attr.is_none() {
									as_account_attr = Some(attr.clone());
								}
								return false; // remove from variant attrs
							},
							"nonce_provider" => {
								if flag_error.is_none() {
									flag_error =
										check_flag_attr(attr, "nonce_provider", has_nonce_provider);
								}
								has_nonce_provider = true;
								nonce_provider_span = Some(attr.span());
								return false;
							},
							"fee_payer" => {
								if flag_error.is_none() {
									flag_error = check_flag_attr(attr, "fee_payer", has_fee_payer);
								}
								has_fee_payer = true;
								fee_payer_span = Some(attr.span());
								return false;
							},
							_ => {},
						}
					}
					true
				});

				if as_account_count > 1 {
					return Err(syn::Error::new(
						variant.ident.span(),
						"Duplicate `#[pallet::as_account(...)]` attribute on variant",
					));
				}

				if let Some(err) = flag_error {
					return Err(err);
				}

				// Validate: nonce_provider/fee_payer require as_account
				if has_nonce_provider && as_account_attr.is_none() {
					return Err(syn::Error::new(
						nonce_provider_span.unwrap(),
						"`#[pallet::nonce_provider]` requires `#[pallet::as_account(...)]` on the same variant",
					));
				}
				if has_fee_payer && as_account_attr.is_none() {
					return Err(syn::Error::new(
						fee_payer_span.unwrap(),
						"`#[pallet::fee_payer]` requires `#[pallet::as_account(...)]` on the same variant",
					));
				}

				if let Some(account_attr) = as_account_attr {
					let expr: syn::Expr = account_attr.parse_args()?;
					account_like_defs.push(OriginAccountLikeDef {
						variant_ident: variant.ident.clone(),
						fields: variant.fields.clone(),
						cfg_attrs: helper::get_item_cfg_attrs(&variant.attrs),
						expr,
						is_nonce_provider: has_nonce_provider,
						is_fee_payer: has_fee_payer,
					});
				}
			}
		}

		// Reject `pallet::as_account`, `pallet::nonce_provider`, and `pallet::fee_payer`
		// on non-enum origins (structs and type aliases).
		if !matches!(item, syn::Item::Enum(_)) {
			let check_attrs = |attrs: &[syn::Attribute]| -> syn::Result<()> {
				for attr in attrs {
					if attr.path().segments.len() == 2 && attr.path().segments[0].ident == "pallet"
					{
						let attr_name = attr.path().segments[1].ident.to_string();
						if matches!(
							attr_name.as_str(),
							"as_account" | "nonce_provider" | "fee_payer"
						) {
							return Err(syn::Error::new(
								attr.span(),
								format!(
									"`#[pallet::{}]` is only supported on enum origin variants",
									attr_name,
								),
							));
						}
					}
				}
				Ok(())
			};
			match item {
				syn::Item::Struct(s) => check_attrs(&s.attrs)?,
				syn::Item::Type(t) => check_attrs(&t.attrs)?,
				_ => {},
			}
		}

		Ok(OriginDef { is_generic, instances, account_like_defs })
	}
}
