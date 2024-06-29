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

pub mod helper;
pub mod pallet;
pub mod pallet_decl;
pub mod runtime_struct;
pub mod runtime_types;

use crate::construct_runtime::parse::Pallet;
use pallet_decl::PalletDeclaration;
use proc_macro2::TokenStream as TokenStream2;
use quote::ToTokens;
use std::collections::HashMap;
use syn::{spanned::Spanned, Ident, Token};

use frame_support_procedural_tools::syn_ext as ext;
use runtime_types::RuntimeType;

mod keyword {
	use syn::custom_keyword;

	custom_keyword!(runtime);
	custom_keyword!(derive);
	custom_keyword!(pallet_index);
	custom_keyword!(disable_call);
	custom_keyword!(disable_unsigned);
}

enum RuntimeAttr {
	Runtime(proc_macro2::Span),
	Derive(proc_macro2::Span, Vec<RuntimeType>),
	PalletIndex(proc_macro2::Span, u8),
	DisableCall(proc_macro2::Span),
	DisableUnsigned(proc_macro2::Span),
}

impl RuntimeAttr {
	fn span(&self) -> proc_macro2::Span {
		match self {
			Self::Runtime(span) => *span,
			Self::Derive(span, _) => *span,
			Self::PalletIndex(span, _) => *span,
			Self::DisableCall(span) => *span,
			Self::DisableUnsigned(span) => *span,
		}
	}
}

impl syn::parse::Parse for RuntimeAttr {
	fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
		input.parse::<syn::Token![#]>()?;
		let content;
		syn::bracketed!(content in input);
		content.parse::<keyword::runtime>()?;
		content.parse::<syn::Token![::]>()?;

		let lookahead = content.lookahead1();
		if lookahead.peek(keyword::runtime) {
			Ok(RuntimeAttr::Runtime(content.parse::<keyword::runtime>()?.span()))
		} else if lookahead.peek(keyword::derive) {
			let _ = content.parse::<keyword::derive>();
			let derive_content;
			syn::parenthesized!(derive_content in content);
			let runtime_types =
				derive_content.parse::<ext::Punctuated<RuntimeType, Token![,]>>()?;
			let runtime_types = runtime_types.inner.into_iter().collect();
			Ok(RuntimeAttr::Derive(derive_content.span(), runtime_types))
		} else if lookahead.peek(keyword::pallet_index) {
			let _ = content.parse::<keyword::pallet_index>();
			let pallet_index_content;
			syn::parenthesized!(pallet_index_content in content);
			let pallet_index = pallet_index_content.parse::<syn::LitInt>()?;
			if !pallet_index.suffix().is_empty() {
				let msg = "Number literal must not have a suffix";
				return Err(syn::Error::new(pallet_index.span(), msg))
			}
			Ok(RuntimeAttr::PalletIndex(pallet_index.span(), pallet_index.base10_parse()?))
		} else if lookahead.peek(keyword::disable_call) {
			Ok(RuntimeAttr::DisableCall(content.parse::<keyword::disable_call>()?.span()))
		} else if lookahead.peek(keyword::disable_unsigned) {
			Ok(RuntimeAttr::DisableUnsigned(content.parse::<keyword::disable_unsigned>()?.span()))
		} else {
			Err(lookahead.error())
		}
	}
}

#[derive(Debug, Clone)]
pub enum AllPalletsDeclaration {
	Implicit(ImplicitAllPalletsDeclaration),
	Explicit(ExplicitAllPalletsDeclaration),
}

/// Declaration of a runtime with some pallet with implicit declaration of parts.
#[derive(Debug, Clone)]
pub struct ImplicitAllPalletsDeclaration {
	pub name: Ident,
	pub pallet_decls: Vec<PalletDeclaration>,
	pub pallet_count: usize,
}

/// Declaration of a runtime with all pallet having explicit declaration of parts.
#[derive(Debug, Clone)]
pub struct ExplicitAllPalletsDeclaration {
	pub name: Ident,
	pub pallets: Vec<Pallet>,
}

pub struct Def {
	pub input: TokenStream2,
	pub item: syn::ItemMod,
	pub runtime_struct: runtime_struct::RuntimeStructDef,
	pub pallets: AllPalletsDeclaration,
	pub runtime_types: Vec<RuntimeType>,
}

impl Def {
	pub fn try_from(mut item: syn::ItemMod) -> syn::Result<Self> {
		let input: TokenStream2 = item.to_token_stream().into();
		let item_span = item.span();
		let items = &mut item
			.content
			.as_mut()
			.ok_or_else(|| {
				let msg = "Invalid runtime definition, expected mod to be inlined.";
				syn::Error::new(item_span, msg)
			})?
			.1;

		let mut runtime_struct = None;
		let mut runtime_types = None;

		let mut indices = HashMap::new();
		let mut names = HashMap::new();

		let mut pallet_decls = vec![];
		let mut pallets = vec![];

		for item in items.iter_mut() {
			let mut pallet_index_and_item = None;

			let mut disable_call = false;
			let mut disable_unsigned = false;

			while let Some(runtime_attr) =
				helper::take_first_item_runtime_attr::<RuntimeAttr>(item)?
			{
				match runtime_attr {
					RuntimeAttr::Runtime(span) if runtime_struct.is_none() => {
						let p = runtime_struct::RuntimeStructDef::try_from(span, item)?;
						runtime_struct = Some(p);
					},
					RuntimeAttr::Derive(_, types) if runtime_types.is_none() => {
						runtime_types = Some(types);
					},
					RuntimeAttr::PalletIndex(span, index) => {
						pallet_index_and_item = if let syn::Item::Type(item) = item {
							Some((index, item.clone()))
						} else {
							let msg = "Invalid runtime::pallet_index, expected type definition";
							return Err(syn::Error::new(span, msg))
						};
					},
					RuntimeAttr::DisableCall(_) => disable_call = true,
					RuntimeAttr::DisableUnsigned(_) => disable_unsigned = true,
					attr => {
						let msg = "Invalid duplicated attribute";
						return Err(syn::Error::new(attr.span(), msg))
					},
				}
			}

			if let Some((pallet_index, pallet_item)) = pallet_index_and_item {
				match *pallet_item.ty.clone() {
					syn::Type::Path(ref path) => {
						let pallet_decl =
							PalletDeclaration::try_from(item.span(), &pallet_item, &path.path)?;

						if let Some(used_pallet) =
							names.insert(pallet_decl.name.clone(), pallet_decl.name.span())
						{
							let msg = "Two pallets with the same name!";

							let mut err = syn::Error::new(used_pallet, &msg);
							err.combine(syn::Error::new(pallet_decl.name.span(), &msg));
							return Err(err)
						}

						pallet_decls.push(pallet_decl);
					},
					syn::Type::TraitObject(syn::TypeTraitObject { bounds, .. }) => {
						let pallet = Pallet::try_from(
							item.span(),
							&pallet_item,
							pallet_index,
							disable_call,
							disable_unsigned,
							&bounds,
						)?;

						if let Some(used_pallet) = indices.insert(pallet.index, pallet.name.clone())
						{
							let msg = format!(
								"Pallet indices are conflicting: Both pallets {} and {} are at index {}",
								used_pallet, pallet.name, pallet.index,
							);
							let mut err = syn::Error::new(used_pallet.span(), &msg);
							err.combine(syn::Error::new(pallet.name.span(), msg));
							return Err(err)
						}

						pallets.push(pallet);
					},
					_ => continue,
				}
			} else {
				if let syn::Item::Type(item) = item {
					let msg = "Missing pallet index for pallet declaration. Please add `#[runtime::pallet_index(...)]`";
					return Err(syn::Error::new(item.span(), &msg))
				}
			}
		}

		let name = item.ident.clone();
		let decl_count = pallet_decls.len();
		let pallets = if decl_count > 0 {
			AllPalletsDeclaration::Implicit(ImplicitAllPalletsDeclaration {
				name,
				pallet_decls,
				pallet_count: decl_count.saturating_add(pallets.len()),
			})
		} else {
			AllPalletsDeclaration::Explicit(ExplicitAllPalletsDeclaration { name, pallets })
		};

		let def = Def {
			input,
			item,
			runtime_struct: runtime_struct.ok_or_else(|| {
				syn::Error::new(item_span,
					"Missing Runtime. Please add a struct inside the module and annotate it with `#[runtime::runtime]`"
				)
			})?,
			pallets,
			runtime_types: runtime_types.ok_or_else(|| {
				syn::Error::new(item_span,
					"Missing Runtime Types. Please annotate the runtime struct with `#[runtime::derive]`"
				)
			})?,
		};

		Ok(def)
	}
}

#[test]
fn runtime_parsing_works() {
	let def = Def::try_from(syn::parse_quote! {
		#[runtime::runtime]
		mod runtime {
			#[runtime::derive(RuntimeCall, RuntimeEvent)]
			#[runtime::runtime]
			pub struct Runtime;

			#[runtime::pallet_index(0)]
			pub type System = frame_system::Pallet<Runtime>;

			#[runtime::pallet_index(1)]
			pub type Pallet1 = pallet1<Instance1>;
		}
	})
	.expect("Failed to parse runtime definition");

	assert_eq!(def.runtime_struct.ident, "Runtime");
}
