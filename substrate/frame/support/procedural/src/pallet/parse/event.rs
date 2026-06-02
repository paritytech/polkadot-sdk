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

/// List of additional token to be used for parsing.
mod keyword {
	syn::custom_keyword!(Event);
	syn::custom_keyword!(pallet);
	syn::custom_keyword!(generate_deposit);
	syn::custom_keyword!(deposit_event);
	syn::custom_keyword!(chain_batch);
	syn::custom_keyword!(capture);
}

/// Definition for pallet event enum.
pub struct EventDef {
	/// The index of event item in pallet module.
	pub index: usize,
	/// The keyword Event used (contains span).
	pub event: keyword::Event,
	/// A set of usage of instance, must be check for consistency with trait.
	pub instances: Vec<helper::InstanceUsage>,
	/// The kind of generic the type `Event` has.
	pub gen_kind: super::GenericKind,
	/// Whether the function `deposit_event` must be generated.
	pub deposit_event: Option<PalletEventDepositAttr>,
	/// Where clause used in event definition.
	pub where_clause: Option<syn::WhereClause>,
	/// The span of the pallet::event attribute.
	pub attr_span: proc_macro2::Span,
	/// Whether `deposit_event` should also capture fields for chain batching.
	pub capture_for_chain_batch: bool,
	/// Fields annotated with `#[chain_batch(capture = "name")]` across all event variants.
	pub captured_fields: Vec<CapturedField>,
}

/// Attribute for a pallet's Event.
///
/// Syntax is:
/// * `#[pallet::generate_deposit($vis fn deposit_event)]`
pub struct PalletEventDepositAttr {
	pub fn_vis: syn::Visibility,
	// Span for the keyword deposit_event
	pub fn_span: proc_macro2::Span,
	// Span of the attribute
	pub span: proc_macro2::Span,
}

/// Parsed `#[chain_batch(capture = "name")]` attribute on an event variant field.
pub struct ChainBatchCaptureAttr {
	pub capture_name: String,
}

/// A single event variant field to be captured during `deposit_event` for chain batching.
pub struct CapturedField {
	pub variant_ident: syn::Ident,
	pub field_ident: syn::Ident,
	pub capture_name: String,
}

impl syn::parse::Parse for PalletEventDepositAttr {
	fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
		input.parse::<syn::Token![#]>()?;
		let content;
		syn::bracketed!(content in input);
		content.parse::<keyword::pallet>()?;
		content.parse::<syn::Token![::]>()?;

		let span = content.parse::<keyword::generate_deposit>()?.span();
		let generate_content;
		syn::parenthesized!(generate_content in content);
		let fn_vis = generate_content.parse::<syn::Visibility>()?;
		generate_content.parse::<syn::Token![fn]>()?;
		let fn_span = generate_content.parse::<keyword::deposit_event>()?.span();

		Ok(PalletEventDepositAttr { fn_vis, span, fn_span })
	}
}

impl syn::parse::Parse for ChainBatchCaptureAttr {
	fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
		input.parse::<keyword::capture>()?;
		input.parse::<syn::Token![=]>()?;
		let capture_name = input.parse::<syn::LitStr>()?.value();
		Ok(ChainBatchCaptureAttr { capture_name })
	}
}

struct PalletEventAttrInfo {
	deposit_event: Option<PalletEventDepositAttr>,
}

impl PalletEventAttrInfo {
	fn from_attrs(attrs: Vec<PalletEventDepositAttr>) -> syn::Result<Self> {
		let mut deposit_event = None;
		for attr in attrs {
			if deposit_event.is_none() {
				deposit_event = Some(attr)
			} else {
				return Err(syn::Error::new(attr.span, "Duplicate attribute"));
			}
		}
		Ok(PalletEventAttrInfo { deposit_event })
	}
}

impl EventDef {
	pub fn try_from(
		attr_span: proc_macro2::Span,
		index: usize,
		item: &mut syn::Item,
	) -> syn::Result<Self> {
		let item = if let syn::Item::Enum(item) = item {
			item
		} else {
			return Err(syn::Error::new(item.span(), "Invalid pallet::event, expected enum item"));
		};

		crate::deprecation::prevent_deprecation_attr_on_outer_enum(&item.attrs)?;

		let capture_for_chain_batch = {
			let pos = item.attrs.iter().position(|attr| {
				attr.path().segments.len() == 2 &&
					attr.path().segments[0].ident == "pallet" &&
					attr.path().segments[1].ident == "capture_for_chain_batch"
			});
			if let Some(idx) = pos {
				item.attrs.remove(idx);
				true
			} else {
				false
			}
		};

		let event_attrs: Vec<PalletEventDepositAttr> =
			helper::take_item_pallet_attrs(&mut item.attrs)?;
		let attr_info = PalletEventAttrInfo::from_attrs(event_attrs)?;
		let deposit_event = attr_info.deposit_event;

		if !matches!(item.vis, syn::Visibility::Public(_)) {
			let msg = "Invalid pallet::event, `Event` must be public";
			return Err(syn::Error::new(item.span(), msg));
		}

		let where_clause = item.generics.where_clause.clone();

		let mut instances = vec![];
		// NOTE: Event is not allowed to be only generic on I because it is not supported
		// by construct_runtime.
		if let Some(u) = helper::check_type_def_optional_gen(&item.generics, item.ident.span())? {
			instances.push(u);
		} else {
			// construct_runtime only allow non generic event for non instantiable pallet.
			instances.push(helper::InstanceUsage { has_instance: false, span: item.ident.span() })
		}

		let has_instance = item.generics.type_params().any(|t| t.ident == "I");
		let has_config = item.generics.type_params().any(|t| t.ident == "T");
		let gen_kind = super::GenericKind::from_gens(has_config, has_instance)
			.expect("Checked by `helper::check_type_def_optional_gen` above");

		let event = syn::parse2::<keyword::Event>(item.ident.to_token_stream())?;

		let mut captured_fields = Vec::new();
		if capture_for_chain_batch {
			for variant in &item.variants {
				if let syn::Fields::Named(fields_named) = &variant.fields {
					for field in &fields_named.named {
						if let Some(ident) = &field.ident {
							for attr in &field.attrs {
								if attr.path().segments.len() == 1 &&
									attr.path().segments[0].ident == "chain_batch"
								{
									if let syn::Meta::List(meta_list) = &attr.meta {
										let tokens = meta_list.tokens.clone();
										if let Ok(capture_attr) =
											syn::parse2::<ChainBatchCaptureAttr>(tokens)
										{
											captured_fields.push(CapturedField {
												variant_ident: variant.ident.clone(),
												field_ident: ident.clone(),
												capture_name: capture_attr.capture_name,
											});
										}
									}
								}
							}
						}
					}
				}
			}
		}

		if capture_for_chain_batch {
			for variant in &mut item.variants {
				if let syn::Fields::Named(fields_named) = &mut variant.fields {
					for field in &mut fields_named.named {
						field.attrs.retain(|attr| {
							!(attr.path().segments.len() == 1 &&
								attr.path().segments[0].ident == "chain_batch")
						});
					}
				}
			}
		}

		Ok(EventDef {
			attr_span,
			index,
			instances,
			deposit_event,
			event,
			gen_kind,
			where_clause,
			capture_for_chain_batch,
			captured_fields,
		})
	}
}
