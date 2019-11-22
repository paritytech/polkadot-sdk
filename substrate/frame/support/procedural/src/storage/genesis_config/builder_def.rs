// Copyright 2017-2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Builder logic definition used to build genesis storage.

use frame_support_procedural_tools::syn_ext as ext;
use proc_macro2::TokenStream;
use syn::spanned::Spanned;
use quote::{quote, quote_spanned};
use super::super::{DeclStorageDefExt, StorageLineTypeDef};

/// Definition of builder blocks, each block insert some value in the storage.
/// They must be called inside externalities, and with `self` being the genesis config.
pub struct BuilderDef {
	/// Contains:
	/// * build block for storage with build attribute.
	/// * build block for storage with config attribute and no build attribute.
	/// * build block for extra genesis build expression.
	pub blocks: Vec<TokenStream>,
	/// The build blocks requires generic traits.
	pub is_generic: bool,
}

impl BuilderDef {
	pub fn from_def(scrate: &TokenStream, def: &DeclStorageDefExt) -> Self {
		let mut blocks = Vec::new();
		let mut is_generic = false;

		for line in def.storage_lines.iter() {
			let storage_struct = &line.storage_struct;
			let storage_trait = &line.storage_trait;
			let value_type = &line.value_type;

			// Contains the data to inset at genesis either from build or config.
			let mut data = None;

			if let Some(builder) = &line.build {
				is_generic |= ext::expr_contains_ident(&builder, &def.module_runtime_generic);
				is_generic |= line.is_generic;

				data = Some(quote_spanned!(builder.span() => &(#builder)(self)));
			} else if let Some(config) = &line.config {
				is_generic |= line.is_generic;

				data = Some(quote!(&self.#config;));
			};

			if let Some(data) = data {
				blocks.push(match &line.storage_type {
					StorageLineTypeDef::Simple(_) => {
						quote!{{
							let v: &#value_type = #data;
							<#storage_struct as #scrate::#storage_trait>::put::<&#value_type>(v);
						}}
					},
					StorageLineTypeDef::Map(map) | StorageLineTypeDef::LinkedMap(map) => {
						let key = &map.key;
						quote!{{
							let data: &#scrate::rstd::vec::Vec<(#key, #value_type)> = #data;
							data.iter().for_each(|(k, v)| {
								<#storage_struct as #scrate::#storage_trait>::insert::<
									&#key, &#value_type
								>(k, v);
							});
						}}
					},
					StorageLineTypeDef::DoubleMap(map) => {
						let key1 = &map.key1;
						let key2 = &map.key2;
						quote!{{
							let data: &#scrate::rstd::vec::Vec<(#key1, #key2, #value_type)> = #data;
							data.iter().for_each(|(k1, k2, v)| {
								<#storage_struct as #scrate::#storage_trait>::insert::<
									&#key1, &#key2, &#value_type
								>(k1, k2, v);
							});
						}}
					},
				});
			}
		}

		if let Some(builder) = def.extra_genesis_build.as_ref() {
			is_generic |= ext::expr_contains_ident(&builder, &def.module_runtime_generic);

			blocks.push(quote_spanned! { builder.span() =>
				let extra_genesis_builder: fn(&Self) = #builder;
				extra_genesis_builder(self);
			});
		}


		Self {
			blocks,
			is_generic,
		}
	}
}
