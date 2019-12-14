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

//! Declaration of genesis config structure and implementation of build storage trait and
//! functions.

use proc_macro2::{TokenStream, Span};
use quote::quote;
use super::{DeclStorageDefExt, instance_trait::DEFAULT_INSTANTIABLE_TRAIT_NAME};
use genesis_config_def::GenesisConfigDef;
use builder_def::BuilderDef;

mod genesis_config_def;
mod builder_def;

const DEFAULT_INSTANCE_NAME: &str = "__GeneratedInstance";

fn decl_genesis_config_and_impl_default(
	scrate: &TokenStream,
	genesis_config: &GenesisConfigDef,
) -> TokenStream {
	let config_fields = genesis_config.fields.iter().map(|field| {
		let (name, typ, attrs) = (&field.name, &field.typ, &field.attrs);
		quote!( #( #[ #attrs] )* pub #name: #typ, )
	});

	let config_field_defaults = genesis_config.fields.iter().map(|field| {
		let (name, default) = (&field.name, &field.default);
		quote!( #name: #default, )
	});

	let serde_bug_bound = if !genesis_config.fields.is_empty() {
		let mut b_ser = String::new();
		let mut b_dser = String::new();

		for typ in genesis_config.fields.iter().map(|c| &c.typ) {
			let typ = quote!( #typ );
			b_ser.push_str(&format!("{} : {}::serde::Serialize, ", typ, scrate));
			b_dser.push_str(&format!("{} : {}::serde::de::DeserializeOwned, ", typ, scrate));
		}

		quote! {
			#[serde(bound(serialize = #b_ser))]
			#[serde(bound(deserialize = #b_dser))]
		}
	} else {
		quote!()
	};

	let genesis_struct_decl = &genesis_config.genesis_struct_decl;
	let genesis_struct = &genesis_config.genesis_struct;
	let genesis_impl = &genesis_config.genesis_impl;
	let genesis_where_clause = &genesis_config.genesis_where_clause;

	quote!(
		#[derive(#scrate::Serialize, #scrate::Deserialize)]
		#[cfg(feature = "std")]
		#[serde(rename_all = "camelCase")]
		#[serde(deny_unknown_fields)]
		#serde_bug_bound
		pub struct GenesisConfig#genesis_struct_decl #genesis_where_clause {
			#( #config_fields )*
		}

		#[cfg(feature = "std")]
		impl#genesis_impl Default for GenesisConfig#genesis_struct #genesis_where_clause {
			fn default() -> Self {
				GenesisConfig {
					#( #config_field_defaults )*
				}
			}
		}
	)
}

fn impl_build_storage(
	scrate: &TokenStream,
	def: &DeclStorageDefExt,
	genesis_config: &GenesisConfigDef,
	builders: &BuilderDef,
) -> TokenStream {
	let runtime_generic = &def.module_runtime_generic;
	let runtime_trait = &def.module_runtime_trait;
	let optional_instance = &def.optional_instance;
	let optional_instance_bound = &def.optional_instance_bound;
	let where_clause = &def.where_clause;

	let inherent_instance = def.optional_instance.clone().unwrap_or_else(|| {
		let name = syn::Ident::new(DEFAULT_INSTANCE_NAME, Span::call_site());
		quote!( #name )
	});
	let inherent_instance_bound = def.optional_instance_bound.clone().unwrap_or_else(|| {
		let bound = syn::Ident::new(DEFAULT_INSTANTIABLE_TRAIT_NAME, Span::call_site());
		quote!( #inherent_instance: #bound )
	});

	let build_storage_impl = quote!(
		<#runtime_generic: #runtime_trait, #inherent_instance_bound>
	);

	let genesis_struct = &genesis_config.genesis_struct;
	let genesis_impl = &genesis_config.genesis_impl;
	let genesis_where_clause = &genesis_config.genesis_where_clause;

	let (
		fn_generic,
		fn_traitinstance,
		fn_where_clause
	) = if !genesis_config.is_generic && builders.is_generic {
		(
			quote!( <#runtime_generic: #runtime_trait, #optional_instance_bound> ),
			quote!( #runtime_generic, #optional_instance ),
			Some(&def.where_clause),
		)
	} else {
		(quote!(), quote!(), None)
	};

	let builder_blocks = &builders.blocks;

	let build_storage_impl_trait = quote!(
		#scrate::sp_runtime::BuildModuleGenesisStorage<#runtime_generic, #inherent_instance>
	);

	quote!{
		#[cfg(feature = "std")]
		impl#genesis_impl GenesisConfig#genesis_struct #genesis_where_clause {
			pub fn build_storage #fn_generic (&self) -> std::result::Result<
				#scrate::sp_runtime::Storage,
				String
			> #fn_where_clause {
				let mut storage = Default::default();
				self.assimilate_storage::<#fn_traitinstance>(&mut storage)?;
				Ok(storage)
			}

			/// Assimilate the storage for this module into pre-existing overlays.
			pub fn assimilate_storage #fn_generic (
				&self,
				storage: &mut #scrate::sp_runtime::Storage,
			) -> std::result::Result<(), String> #fn_where_clause {
				#scrate::BasicExternalities::execute_with_storage(storage, || {
					#( #builder_blocks )*
					Ok(())
				})
			}
		}

		#[cfg(feature = "std")]
		impl#build_storage_impl #build_storage_impl_trait for GenesisConfig#genesis_struct
			#where_clause
		{
			fn build_module_genesis_storage(
				&self,
				storage: &mut #scrate::sp_runtime::Storage,
			) -> std::result::Result<(), String> {
				self.assimilate_storage::<#fn_traitinstance> (storage)
			}
		}
	}
}

pub fn genesis_config_and_build_storage(
	scrate: &TokenStream,
	def: &DeclStorageDefExt,
) -> TokenStream {
	let builders = BuilderDef::from_def(scrate, def);
	if !builders.blocks.is_empty() {
		let genesis_config = match GenesisConfigDef::from_def(def) {
			Ok(genesis_config) => genesis_config,
			Err(err) => return err.to_compile_error(),
		};
		let decl_genesis_config_and_impl_default =
			decl_genesis_config_and_impl_default(scrate, &genesis_config);
		let impl_build_storage = impl_build_storage(scrate, def, &genesis_config, &builders);

		quote!{
			#decl_genesis_config_and_impl_default
			#impl_build_storage
		}
	} else {
		quote!()
	}
}
