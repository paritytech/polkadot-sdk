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

use crate::{
	construct_runtime::parse::{Pallet, PalletPart, PalletPartKeyword, PalletPath},
	runtime::parse::PalletDeclaration,
};
use frame_support_procedural_tools::get_doc_literals;
use quote::ToTokens;
use syn::{punctuated::Punctuated, token, Error};

impl Pallet {
	pub fn try_from(
		attr_span: proc_macro2::Span,
		item: &syn::ItemType,
		pallet_index: u8,
		disable_call: bool,
		disable_unsigned: bool,
		bounds: &Punctuated<syn::TypeParamBound, token::Plus>,
	) -> syn::Result<Self> {
		let name = item.ident.clone();

		let mut pallet_path = None;
		let mut pallet_parts = vec![];

		for (index, bound) in bounds.into_iter().enumerate() {
			if let syn::TypeParamBound::Trait(syn::TraitBound { path, .. }) = bound {
				if index == 0 {
					pallet_path = Some(PalletPath { inner: path.clone() });
				} else {
					let pallet_part = syn::parse2::<PalletPart>(bound.into_token_stream())?;
					pallet_parts.push(pallet_part);
				}
			} else {
				return Err(Error::new(
					attr_span,
					"Invalid pallet declaration, expected a path or a trait object",
				))
			};
		}

		let mut path = pallet_path.ok_or(Error::new(
			attr_span,
			"Invalid pallet declaration, expected a path or a trait object",
		))?;

		let PalletDeclaration { path: inner, instance, .. } =
			PalletDeclaration::try_from(attr_span, item, &path.inner)?;

		path = PalletPath { inner };

		pallet_parts = pallet_parts
			.into_iter()
			.filter(|part| {
				if let (true, &PalletPartKeyword::Call(_)) = (disable_call, &part.keyword) {
					false
				} else if let (true, &PalletPartKeyword::ValidateUnsigned(_)) =
					(disable_unsigned, &part.keyword)
				{
					false
				} else {
					true
				}
			})
			.collect();

		let cfg_pattern = vec![];

		let docs = get_doc_literals(&item.attrs);

		Ok(Pallet {
			is_expanded: true,
			name,
			index: pallet_index,
			path,
			instance,
			cfg_pattern,
			pallet_parts,
			docs,
		})
	}
}

#[test]
fn pallet_parsing_works() {
	use syn::{parse_quote, ItemType};

	let item: ItemType = parse_quote! {
		pub type System = frame_system + Call;
	};
	let ItemType { ty, .. } = item.clone();
	let syn::Type::TraitObject(syn::TypeTraitObject { bounds, .. }) = *ty else {
		panic!("Expected a trait object");
	};

	let index = 0;
	let pallet =
		Pallet::try_from(proc_macro2::Span::call_site(), &item, index, false, false, &bounds)
			.unwrap();

	assert_eq!(pallet.name.to_string(), "System");
	assert_eq!(pallet.index, index);
	assert_eq!(pallet.path.to_token_stream().to_string(), "frame_system");
	assert_eq!(pallet.instance, None);
}

#[test]
fn pallet_parsing_works_with_instance() {
	use syn::{parse_quote, ItemType};

	let item: ItemType = parse_quote! {
		pub type System = frame_system<Instance1> + Call;
	};
	let ItemType { ty, .. } = item.clone();
	let syn::Type::TraitObject(syn::TypeTraitObject { bounds, .. }) = *ty else {
		panic!("Expected a trait object");
	};

	let index = 0;
	let pallet =
		Pallet::try_from(proc_macro2::Span::call_site(), &item, index, false, false, &bounds)
			.unwrap();

	assert_eq!(pallet.name.to_string(), "System");
	assert_eq!(pallet.index, index);
	assert_eq!(pallet.path.to_token_stream().to_string(), "frame_system");
	assert_eq!(pallet.instance, Some(parse_quote! { Instance1 }));
}

#[test]
fn pallet_parsing_works_with_pallet() {
	use syn::{parse_quote, ItemType};

	let item: ItemType = parse_quote! {
		pub type System = frame_system::Pallet<Runtime> + Call;
	};
	let ItemType { ty, .. } = item.clone();
	let syn::Type::TraitObject(syn::TypeTraitObject { bounds, .. }) = *ty else {
		panic!("Expected a trait object");
	};

	let index = 0;
	let pallet =
		Pallet::try_from(proc_macro2::Span::call_site(), &item, index, false, false, &bounds)
			.unwrap();

	assert_eq!(pallet.name.to_string(), "System");
	assert_eq!(pallet.index, index);
	assert_eq!(pallet.path.to_token_stream().to_string(), "frame_system");
	assert_eq!(pallet.instance, None);
}

#[test]
fn pallet_parsing_works_with_instance_and_pallet() {
	use syn::{parse_quote, ItemType};

	let item: ItemType = parse_quote! {
		pub type System = frame_system::Pallet<Runtime, Instance1> + Call;
	};
	let ItemType { ty, .. } = item.clone();
	let syn::Type::TraitObject(syn::TypeTraitObject { bounds, .. }) = *ty else {
		panic!("Expected a trait object");
	};

	let index = 0;
	let pallet =
		Pallet::try_from(proc_macro2::Span::call_site(), &item, index, false, false, &bounds)
			.unwrap();

	assert_eq!(pallet.name.to_string(), "System");
	assert_eq!(pallet.index, index);
	assert_eq!(pallet.path.to_token_stream().to_string(), "frame_system");
	assert_eq!(pallet.instance, Some(parse_quote! { Instance1 }));
}
