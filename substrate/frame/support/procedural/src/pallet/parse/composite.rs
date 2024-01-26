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

pub mod keyword {
	use super::*;

	syn::custom_keyword!(FreezeReason);
	syn::custom_keyword!(HoldReason);
	syn::custom_keyword!(LockId);
	syn::custom_keyword!(SlashReason);
	syn::custom_keyword!(Task);

	pub enum CompositeKeyword {
		FreezeReason(FreezeReason),
		HoldReason(HoldReason),
		LockId(LockId),
		SlashReason(SlashReason),
		Task(Task),
	}

	impl ToTokens for CompositeKeyword {
		fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
			use CompositeKeyword::*;
			match self {
				FreezeReason(inner) => inner.to_tokens(tokens),
				HoldReason(inner) => inner.to_tokens(tokens),
				LockId(inner) => inner.to_tokens(tokens),
				SlashReason(inner) => inner.to_tokens(tokens),
				Task(inner) => inner.to_tokens(tokens),
			}
		}
	}

	impl syn::parse::Parse for CompositeKeyword {
		fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
			let lookahead = input.lookahead1();
			if lookahead.peek(FreezeReason) {
				Ok(Self::FreezeReason(input.parse()?))
			} else if lookahead.peek(HoldReason) {
				Ok(Self::HoldReason(input.parse()?))
			} else if lookahead.peek(LockId) {
				Ok(Self::LockId(input.parse()?))
			} else if lookahead.peek(SlashReason) {
				Ok(Self::SlashReason(input.parse()?))
			} else if lookahead.peek(Task) {
				Ok(Self::Task(input.parse()?))
			} else {
				Err(lookahead.error())
			}
		}
	}

	impl std::fmt::Display for CompositeKeyword {
		fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
			use CompositeKeyword::*;
			write!(
				f,
				"{}",
				match self {
					FreezeReason(_) => "FreezeReason",
					HoldReason(_) => "HoldReason",
					Task(_) => "Task",
					LockId(_) => "LockId",
					SlashReason(_) => "SlashReason",
				}
			)
		}
	}
}

pub struct CompositeDef {
	/// The index of the CompositeDef item in the pallet module.
	pub index: usize,
	/// The composite keyword used (contains span).
	pub composite_keyword: keyword::CompositeKeyword,

	/// Name of the associated type.
	pub ident: syn::Ident,
	/// Type parameters and where clause attached to a declaration of the pallet::composite_enum.
	pub generics: syn::Generics,
	/// The span of the pallet::composite_enum attribute.
	pub attr_span: proc_macro2::Span,

	/// Variant count of the pallet::composite_enum.
	pub variant_count: u32,
}

impl CompositeDef {
	pub fn try_from(
		attr_span: proc_macro2::Span,
		index: usize,
		scrate: &syn::Path,
		item: &mut syn::Item,
	) -> syn::Result<Self> {
		let item = if let syn::Item::Enum(item) = item {
			// check variants: composite enums support only field-less enum variants. This is
			// because fields can introduce too many possibilities, making it challenging to compute
			// a fixed variant count.
			for variant in &item.variants {
				match variant.fields {
					syn::Fields::Named(_) | syn::Fields::Unnamed(_) =>
						return Err(syn::Error::new(
							variant.ident.span(),
							"The composite enum does not support variants with fields!",
						)),
					syn::Fields::Unit => (),
				}
			}
			item
		} else {
			return Err(syn::Error::new(
				item.span(),
				"Invalid pallet::composite_enum, expected enum item",
			))
		};

		if !matches!(item.vis, syn::Visibility::Public(_)) {
			let msg = format!("Invalid pallet::composite_enum, `{}` must be public", item.ident);
			return Err(syn::Error::new(item.span(), msg))
		}

		let has_instance = if item.generics.params.first().is_some() {
			helper::check_config_def_gen(&item.generics, item.ident.span())?;
			true
		} else {
			false
		};

		let has_derive_attr = item.attrs.iter().any(|attr| {
			if let syn::Meta::List(syn::MetaList { path, .. }) = &attr.meta {
				path.get_ident().map(|ident| ident == "derive").unwrap_or(false)
			} else {
				false
			}
		});

		if !has_derive_attr {
			let derive_attr: syn::Attribute = syn::parse_quote! {
				#[derive(
					Copy, Clone, Eq, PartialEq,
					#scrate::__private::codec::Encode, #scrate::__private::codec::Decode, #scrate::__private::codec::MaxEncodedLen,
					#scrate::__private::scale_info::TypeInfo,
					#scrate::__private::RuntimeDebug,
				)]
			};
			item.attrs.push(derive_attr);
		}

		if has_instance {
			item.attrs.push(syn::parse_quote! {
				#[scale_info(skip_type_params(I))]
			});

			item.variants.push(syn::parse_quote! {
				#[doc(hidden)]
				#[codec(skip)]
				__Ignore(
					#scrate::__private::sp_std::marker::PhantomData<I>,
				)
			});
		}

		let composite_keyword =
			syn::parse2::<keyword::CompositeKeyword>(item.ident.to_token_stream())?;

		Ok(CompositeDef {
			index,
			composite_keyword,
			attr_span,
			generics: item.generics.clone(),
			variant_count: item.variants.len() as u32,
			ident: item.ident.clone(),
		})
	}
}
