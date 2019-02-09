// Copyright 2017-2018 Parity Technologies (UK) Ltd.
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

// tag::description[]
//! `decl_storage` macro transformation
// end::description[]

use srml_support_procedural_tools::syn_ext as ext;
use srml_support_procedural_tools::{generate_crate_access, generate_hidden_includes, clean_type_string};

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;

use syn::{
	Ident,
	GenericParam,
	spanned::Spanned,
	parse::{
		Error,
		Result,
	},
	parse_macro_input,
};
use quote::quote;

use super::*;

// try macro but returning tokenized error
macro_rules! try_tok(( $expre : expr ) => {
	match $expre {
		Ok(r) => r,
		Err (err) => {
			return err.to_compile_error().into()
		}
	}
});

pub fn decl_storage_impl(input: TokenStream) -> TokenStream {
	let def = parse_macro_input!(input as StorageDefinition);

	let StorageDefinition {
		hidden_crate,
		visibility,
		ident: storetype,
		module_ident,
		mod_param: strait,
		crate_ident: cratename,
		content: ext::Braces { content: storage_lines, ..},
		extra_genesis,
		..
	} = def;
	let hidden_crate_name = hidden_crate.map(|rc| rc.ident.content).map(|i| i.to_string())
		.unwrap_or_else(|| "decl_storage".to_string());
	let scrate = generate_crate_access(&hidden_crate_name, "srml-support");
	let scrate_decl = generate_hidden_includes(
		&hidden_crate_name,
		"srml-support",
		"srml_support",
	);

	let (
		traitinstance,
		traittypes,
	) = if let GenericParam::Type(syn::TypeParam {ident, bounds, ..}) = strait {
		(ident, bounds)
	} else {
		return try_tok!(Err(Error::new(strait.span(), "Missing declare store generic params")));
	};

	let traittype =	if let Some(traittype) = traittypes.first() {
		traittype.into_value()
	} else {
		return try_tok!(Err(Error::new(traittypes.span(), "Trait bound expected")));
	};

	let extra_genesis = try_tok!(decl_store_extra_genesis(
		&scrate,
		&traitinstance,
		&traittype,
		&storage_lines,
		&extra_genesis,
	));
	let decl_storage_items = decl_storage_items(
		&scrate,
		&traitinstance,
		&traittype,
		&cratename,
		&storage_lines,
	);
	let decl_store_items = decl_store_items(
		&storage_lines,
	);
	let impl_store_items = impl_store_items(
		&traitinstance,
		&storage_lines,
	);
	let impl_store_fns = impl_store_fns(
		&scrate,
		&traitinstance,
		&storage_lines,
	);
	let (store_default_struct, store_functions_to_metadata) = store_functions_to_metadata(
		&scrate,
		&traitinstance,
		&traittype,
		&storage_lines,
	);
	let cratename_string = cratename.to_string();
	let expanded = quote! {
		#scrate_decl
		#decl_storage_items
		#visibility trait #storetype {
			#decl_store_items
		}
		#store_default_struct
		impl<#traitinstance: #traittype> #storetype for #module_ident<#traitinstance> {
			#impl_store_items
		}
		impl<#traitinstance: 'static + #traittype> #module_ident<#traitinstance> {
			#impl_store_fns
			pub fn store_metadata() -> #scrate::storage::generator::StorageMetadata {
				#scrate::storage::generator::StorageMetadata {
					functions: #scrate::storage::generator::DecodeDifferent::Encode(#store_functions_to_metadata) ,
				}
			}
			pub fn store_metadata_functions() -> &'static [#scrate::storage::generator::StorageFunctionMetadata] {
				#store_functions_to_metadata
			}
			pub fn store_metadata_name() -> &'static str {
				#cratename_string
			}

		}

		#extra_genesis

	};

	expanded.into()
}

fn decl_store_extra_genesis(
	scrate: &TokenStream2,
	traitinstance: &Ident,
	traittype: &syn::TypeParamBound,
	storage_lines: &ext::Punctuated<DeclStorageLine, Token![;]>,
	extra_genesis: &Option<AddExtraGenesis>,
) -> Result<TokenStream2> {

	let mut is_trait_needed = false;
	let mut has_trait_field = false;
	let mut serde_complete_bound = std::collections::HashSet::new();
	let mut config_field = TokenStream2::new();
	let mut config_field_default = TokenStream2::new();
	let mut builders = TokenStream2::new();
	for sline in storage_lines.inner.iter() {

		let DeclStorageLine {
			name,
			getter,
			config,
			build,
			storage_type,
			default_value,
			..
		} = sline;

		let type_infos = get_type_infos(storage_type);

		let mut opt_build;
		// need build line
		if let (Some(ref getter), Some(ref config)) = (getter, config) {
			let ident = if let Some(ident) = config.expr.content.as_ref() {
				quote!( #ident )
			} else {
				let ident = &getter.getfn.content;
				quote!( #ident )
			};
			if type_infos.is_simple && ext::has_parametric_type(type_infos.full_type, traitinstance) {
				is_trait_needed = true;
				has_trait_field = true;
			}
			for t in ext::get_non_bound_serde_derive_types(type_infos.full_type, &traitinstance).into_iter() {
				serde_complete_bound.insert(t);
			}
			if let Some(kt) = type_infos.map_key {
				for t in ext::get_non_bound_serde_derive_types(kt, &traitinstance).into_iter() {
					serde_complete_bound.insert(t);
				}
			}
			let storage_type = type_infos.typ.clone();
			config_field.extend(quote!( pub #ident: #storage_type, ));
			opt_build = Some(build.as_ref().map(|b| &b.expr.content).map(|b|quote!( #b ))
				.unwrap_or_else(|| quote!( (|config: &GenesisConfig<#traitinstance>| config.#ident.clone()) )));
			let fielddefault = default_value.inner.as_ref().map(|d| &d.expr).map(|d|
				if type_infos.is_option {
					quote!( #d.unwrap_or_default() )
				} else {
					quote!( #d )
				}).unwrap_or_else(|| quote!( Default::default() ));
					config_field_default.extend(quote!( #ident: #fielddefault, ));

		} else {
			opt_build = build.as_ref().map(|b| &b.expr.content).map(|b| quote!( #b ));
		}

		let typ = type_infos.typ;
		if let Some(builder) = opt_build {
			is_trait_needed = true;
			if type_infos.is_simple {
				builders.extend(quote!{{
					use #scrate::codec::Encode;
					let v = (#builder)(&self);
					r.insert(Self::hash(
						<#name<#traitinstance> as #scrate::storage::generator::StorageValue<#typ>>::key()
						).to_vec(), v.encode());
				}});
			} else {
				let kty = type_infos.map_key.clone().expect("is not simple; qed");
				builders.extend(quote!{{
					use #scrate::codec::Encode;
					let data = (#builder)(&self);
					for (k, v) in data.into_iter() {
						let key = <#name<#traitinstance> as #scrate::storage::generator::StorageMap<#kty, #typ>>::key_for(&k);
						r.insert(Self::hash(&key[..]).to_vec(), v.encode());
					}
				}});
			}
		}

	}

	let mut has_scall = false;
	let mut scall = quote!{ ( |_, _, _| {} ) };
	let mut genesis_extrafields = TokenStream2::new();
	let mut genesis_extrafields_default = TokenStream2::new();

	// extra genesis
	if let Some(eg) = extra_genesis {
		for ex_content in eg.content.content.lines.inner.iter() {
			match ex_content {
				AddExtraGenesisLineEnum::AddExtraGenesisLine(AddExtraGenesisLine {
					attrs,
					extra_field,
					extra_type,
					default_value,
					..
				}) => {
					if ext::has_parametric_type(&extra_type, traitinstance) {
						is_trait_needed = true;
						has_trait_field = true;
					}

					for t in ext::get_non_bound_serde_derive_types(extra_type, &traitinstance).into_iter() {
						serde_complete_bound.insert(t);
					}

					let extrafield = &extra_field.content;
					genesis_extrafields.extend(quote!{
						#attrs pub #extrafield: #extra_type,
					});
					let extra_default = default_value.inner.as_ref().map(|d| &d.expr).map(|e| quote!{ #e })
						.unwrap_or_else(|| quote!( Default::default() ));
					genesis_extrafields_default.extend(quote!{
						#extrafield: #extra_default,
					});
				},
				AddExtraGenesisLineEnum::AddExtraGenesisBuild(DeclStorageBuild{ expr, .. }) => {
					if has_scall {
						return Err(Error::new(expr.span(), "Only one build expression allowed for extra genesis"));
					}
					let content = &expr.content;
					scall = quote!( ( #content ) );
					has_scall = true;
				},
			}
		}
	}


	let serde_bug_bound = if !serde_complete_bound.is_empty() {
		let mut b_ser = String::new();
		let mut b_dser = String::new();
		serde_complete_bound.into_iter().for_each(|bound| {
			let stype = quote!(#bound);
			b_ser.push_str(&format!("{} : {}::serde::Serialize, ", stype, scrate));
			b_dser.push_str(&format!("{} : {}::serde::de::DeserializeOwned, ", stype, scrate));
		});

		quote! {
			#[serde(bound(serialize = #b_ser))]
			#[serde(bound(deserialize = #b_dser))]
		}
	} else {
		quote!()
	};

	let is_extra_genesis_needed = has_scall
		|| !config_field.is_empty()
		|| !genesis_extrafields.is_empty()
		|| !builders.is_empty();
	Ok(if is_extra_genesis_needed {
		let (fparam, sparam, ph_field, ph_default) = if is_trait_needed {
			if has_trait_field {
				// no phantom data required
				(
					quote!(<#traitinstance: #traittype>),
					quote!(<#traitinstance>),
					quote!(),
					quote!(),
				)
			} else {
				// need phantom data
				(
					quote!(<#traitinstance: #traittype>),
					quote!(<#traitinstance>),

					quote!{
						#[serde(skip)]
						pub _genesis_phantom_data: #scrate::storage::generator::PhantomData<#traitinstance>,
					},
					quote!{
						_genesis_phantom_data: Default::default(),
					},
				)
			}
		} else {
			// do not even need type parameter
			(quote!(), quote!(), quote!(), quote!())
		};
		quote!{

			#[derive(#scrate::Serialize, #scrate::Deserialize)]
			#[cfg(feature = "std")]
			#[serde(rename_all = "camelCase")]
			#[serde(deny_unknown_fields)]
			#serde_bug_bound
			pub struct GenesisConfig#fparam {
				#ph_field
				#config_field
				#genesis_extrafields
			}

			#[cfg(feature = "std")]
			impl#fparam Default for GenesisConfig#sparam {
				fn default() -> Self {
					GenesisConfig {
						#ph_default
						#config_field_default
						#genesis_extrafields_default
					}
				}
			}

			#[cfg(feature = "std")]
			impl#fparam #scrate::runtime_primitives::BuildStorage for GenesisConfig#sparam {

				fn build_storage(self) -> ::std::result::Result<(#scrate::runtime_primitives::StorageMap, #scrate::runtime_primitives::ChildrenStorageMap), String> {
					let mut r: #scrate::runtime_primitives::StorageMap = Default::default();
					let mut c: #scrate::runtime_primitives::ChildrenStorageMap = Default::default();

					#builders

					#scall(&mut r, &mut c, &self);

					Ok((r, c))
				}
			}
		}
	} else {
		quote!()
	})
}

fn decl_storage_items(
	scrate: &TokenStream2,
	traitinstance: &Ident,
	traittype: &syn::TypeParamBound,
	cratename: &Ident,
	storage_lines: &ext::Punctuated<DeclStorageLine, Token![;]>,
) -> TokenStream2 {

	let mut impls = TokenStream2::new();
	for sline in storage_lines.inner.iter() {
		let DeclStorageLine {
			name,
			storage_type,
			default_value,
			visibility,
			..
		} = sline;

		let type_infos = get_type_infos(storage_type);
		let gettype = type_infos.full_type;
		let fielddefault = default_value.inner.as_ref().map(|d| &d.expr).map(|d| quote!( #d ))
			.unwrap_or_else(|| quote!{ Default::default() });

		let typ = type_infos.typ;

		let option_simple_1 = if !type_infos.is_option {
			// raw type case
			quote!( unwrap_or_else )
		} else {
			// Option<> type case
			quote!( or_else )
		};
		let implementation = if type_infos.is_simple {
			let mutate_impl = if !type_infos.is_option {
				quote!{
					<Self as #scrate::storage::generator::StorageValue<#typ>>::put(&val, storage)
				}
			} else {
				quote!{
					match val {
						Some(ref val) => <Self as #scrate::storage::generator::StorageValue<#typ>>::put(&val, storage),
						None => <Self as #scrate::storage::generator::StorageValue<#typ>>::kill(storage),
					}
				}
			};

			let key_string = cratename.to_string() + " " + &name.to_string();
			// generator for value
			quote!{

				#visibility struct #name<#traitinstance: #traittype>(#scrate::storage::generator::PhantomData<#traitinstance>);

				impl<#traitinstance: #traittype> #scrate::storage::generator::StorageValue<#typ> for #name<#traitinstance> {
					type Query = #gettype;

					/// Get the storage key.
					fn key() -> &'static [u8] {
						#key_string.as_bytes()
					}

					/// Load the value from the provided storage instance.
					fn get<S: #scrate::GenericStorage>(storage: &S) -> Self::Query {
						storage.get(<#name<#traitinstance> as #scrate::storage::generator::StorageValue<#typ>>::key())
							.#option_simple_1(|| #fielddefault)
					}

					/// Take a value from storage, removing it afterwards.
					fn take<S: #scrate::GenericStorage>(storage: &S) -> Self::Query {
						storage.take(<#name<#traitinstance> as #scrate::storage::generator::StorageValue<#typ>>::key())
							.#option_simple_1(|| #fielddefault)
					}

					/// Mutate the value under a key.
					fn mutate<R, F: FnOnce(&mut Self::Query) -> R, S: #scrate::GenericStorage>(f: F, storage: &S) -> R {
						let mut val = <Self as #scrate::storage::generator::StorageValue<#typ>>::get(storage);

						let ret = f(&mut val);
						#mutate_impl ;
						ret
					}
				}

			}
		} else {
			let kty = type_infos.map_key.expect("is not simple; qed");
			let mutate_impl = if !type_infos.is_option {
				quote!{
					<Self as #scrate::storage::generator::StorageMap<#kty, #typ>>::insert(key, &val, storage)
				}
			} else {
				quote!{
					match val {
						Some(ref val) => <Self as #scrate::storage::generator::StorageMap<#kty, #typ>>::insert(key, &val, storage),
						None => <Self as #scrate::storage::generator::StorageMap<#kty, #typ>>::remove(key, storage),
					}
				}
			};
			let prefix_string = cratename.to_string() + " " + &name.to_string();
			// generator for map
			quote!{
				#visibility struct #name<#traitinstance: #traittype>(#scrate::storage::generator::PhantomData<#traitinstance>);

				impl<#traitinstance: #traittype> #scrate::storage::generator::StorageMap<#kty, #typ> for #name<#traitinstance> {
					type Query = #gettype;

					/// Get the prefix key in storage.
					fn prefix() -> &'static [u8] {
						#prefix_string.as_bytes()
					}

					/// Get the storage key used to fetch a value corresponding to a specific key.
					fn key_for(x: &#kty) -> #scrate::rstd::vec::Vec<u8> {
						let mut key = <#name<#traitinstance> as #scrate::storage::generator::StorageMap<#kty, #typ>>::prefix().to_vec();
						#scrate::codec::Encode::encode_to(x, &mut key);
						key
					}

					/// Load the value associated with the given key from the map.
					fn get<S: #scrate::GenericStorage>(key: &#kty, storage: &S) -> Self::Query {
						let key = <#name<#traitinstance> as #scrate::storage::generator::StorageMap<#kty, #typ>>::key_for(key);
						storage.get(&key[..]).#option_simple_1(|| #fielddefault)
					}

					/// Take the value, reading and removing it.
					fn take<S: #scrate::GenericStorage>(key: &#kty, storage: &S) -> Self::Query {
						let key = <#name<#traitinstance> as #scrate::storage::generator::StorageMap<#kty, #typ>>::key_for(key);
						storage.take(&key[..]).#option_simple_1(|| #fielddefault)
					}

					/// Mutate the value under a key
					fn mutate<R, F: FnOnce(&mut Self::Query) -> R, S: #scrate::GenericStorage>(key: &#kty, f: F, storage: &S) -> R {
						let mut val = <Self as #scrate::storage::generator::StorageMap<#kty, #typ>>::take(key, storage);

						let ret = f(&mut val);
						#mutate_impl ;
						ret
					}

				}

			}
		};
		impls.extend(implementation)
	}
	impls
}


fn decl_store_items(
	storage_lines: &ext::Punctuated<DeclStorageLine, Token![;]>,
) -> TokenStream2 {
	storage_lines.inner.iter().map(|sline| &sline.name)
		.fold(TokenStream2::new(), |mut items, name| {
		items.extend(quote!(type #name;));
		items
	})
}

fn impl_store_items(
	traitinstance: &Ident,
	storage_lines: &ext::Punctuated<DeclStorageLine, Token![;]>,
) -> TokenStream2 {
	storage_lines.inner.iter().map(|sline| &sline.name)
		.fold(TokenStream2::new(), |mut items, name| {
		items.extend(quote!(type #name = #name<#traitinstance>;));
		items
	})
}

fn impl_store_fns(
	scrate: &TokenStream2,
	traitinstance: &Ident,
	storage_lines: &ext::Punctuated<DeclStorageLine, Token![;]>,
) -> TokenStream2 {
	let mut items = TokenStream2::new();
	for sline in storage_lines.inner.iter() {
		let DeclStorageLine {
			name,
			getter,
			storage_type,
			..
		} = sline;

		if let Some(getter) = getter {
			let get_fn = &getter.getfn.content;

			let type_infos = get_type_infos(storage_type);
			let gettype = type_infos.full_type;

			let typ = type_infos.typ;
			let item = if type_infos.is_simple {
				quote!{
					pub fn #get_fn() -> #gettype {
						<#name<#traitinstance> as #scrate::storage::generator::StorageValue<#typ>> :: get(&#scrate::storage::RuntimeStorage)
					}
				}
			} else {
				let kty = type_infos.map_key.expect("is not simple; qed");
				// map
				quote!{
					pub fn #get_fn<K: #scrate::storage::generator::Borrow<#kty>>(key: K) -> #gettype {
						<#name<#traitinstance> as #scrate::storage::generator::StorageMap<#kty, #typ>> :: get(key.borrow(), &#scrate::storage::RuntimeStorage)
					}
				}
			};
			items.extend(item);
		}
	}
	items
}

fn store_functions_to_metadata (
	scrate: &TokenStream2,
	traitinstance: &Ident,
	traittype: &syn::TypeParamBound,
	storage_lines: &ext::Punctuated<DeclStorageLine, Token![;]>,
) -> (TokenStream2, TokenStream2) {

	let mut items = TokenStream2::new();
	let mut default_getter_struct_def = TokenStream2::new();
	for sline in storage_lines.inner.iter() {
		let DeclStorageLine {
			attrs,
			name,
			storage_type,
			default_value,
			..
		} = sline;

		let type_infos = get_type_infos(storage_type);
		let gettype = type_infos.full_type;

		let typ = type_infos.typ;
		let stype = if type_infos.is_simple {
			let styp = clean_type_string(&typ.to_string());
			quote!{
				#scrate::storage::generator::StorageFunctionType::Plain(
					#scrate::storage::generator::DecodeDifferent::Encode(#styp),
				)
			}
		} else {
			let kty = type_infos.map_key.expect("is not simple; qed");
			let kty = clean_type_string(&quote!(#kty).to_string());
			let styp = clean_type_string(&typ.to_string());
			quote!{
				#scrate::storage::generator::StorageFunctionType::Map {
					key: #scrate::storage::generator::DecodeDifferent::Encode(#kty),
					value: #scrate::storage::generator::DecodeDifferent::Encode(#styp),
				}
			}
		};
		let modifier = if type_infos.is_option {
			quote!{
				#scrate::storage::generator::StorageFunctionModifier::Optional
			}
		} else {
			quote!{
				#scrate::storage::generator::StorageFunctionModifier::Default
			}
		};
		let default = default_value.inner.as_ref().map(|d| &d.expr)
			.map(|d| {
				quote!( #d )
			})
			.unwrap_or_else(|| quote!( Default::default() ));
		let mut docs = TokenStream2::new();
		for attr in attrs.inner.iter().filter_map(|v| v.interpret_meta()) {
			if let syn::Meta::NameValue(syn::MetaNameValue{
				ref ident,
				ref lit,
				..
			}) = attr {
				if ident == "doc" {
					docs.extend(quote!(#lit,));
				}
			}
		}
		let str_name = name.to_string();
		let struct_name = proc_macro2::Ident::new(&("__GetByteStruct".to_string() + &str_name), name.span());
		let cache_name = proc_macro2::Ident::new(&("__CACHE_GET_BYTE_STRUCT_".to_string() + &str_name), name.span());
		let item = quote! {
			#scrate::storage::generator::StorageFunctionMetadata {
				name: #scrate::storage::generator::DecodeDifferent::Encode(#str_name),
				modifier: #modifier,
				ty: #stype,
				default: #scrate::storage::generator::DecodeDifferent::Encode(
					#scrate::storage::generator::DefaultByteGetter(
						&#struct_name::<#traitinstance>(#scrate::rstd::marker::PhantomData)
					)
				),
				documentation: #scrate::storage::generator::DecodeDifferent::Encode(&[ #docs ]),
			},
		};
		items.extend(item);
		let def_get = quote! {
			pub struct #struct_name<#traitinstance>(pub #scrate::rstd::marker::PhantomData<#traitinstance>);
			#[cfg(feature = "std")]
			#[allow(non_upper_case_globals)]
			static #cache_name: #scrate::once_cell::sync::OnceCell<#scrate::rstd::vec::Vec<u8>> = #scrate::once_cell::sync::OnceCell::INIT;
			#[cfg(feature = "std")]
			impl<#traitinstance: #traittype> #scrate::storage::generator::DefaultByte for #struct_name<#traitinstance> {
				fn default_byte(&self) -> #scrate::rstd::vec::Vec<u8> {
					use #scrate::codec::Encode;
					#cache_name.get_or_init(|| {
						let def_val: #gettype = #default;
						<#gettype as Encode>::encode(&def_val)
					}).clone()
				}
			}
			#[cfg(not(feature = "std"))]
			impl<#traitinstance: #traittype> #scrate::storage::generator::DefaultByte for #struct_name<#traitinstance> {
				fn default_byte(&self) -> #scrate::rstd::vec::Vec<u8> {
					use #scrate::codec::Encode;
					let def_val: #gettype = #default;
					<#gettype as Encode>::encode(&def_val)
				}
			}
		};
		default_getter_struct_def.extend(def_get);
	}
	(default_getter_struct_def, quote!{
		{
			&[
				#items
			]
		}
	})
}


struct DeclStorageTypeInfos<'a> {
	pub is_simple: bool,
	pub full_type: &'a syn::Type,
	pub is_option: bool,
	pub typ: TokenStream2,
	pub map_key: Option<&'a syn::Type>,
}

fn get_type_infos(storage_type: &DeclStorageType) -> DeclStorageTypeInfos {
	let (is_simple, extracted_type, map_key, full_type) = match storage_type {
		DeclStorageType::Simple(ref st) => (true, ext::extract_type_option(st), None, st),
		DeclStorageType::Map(ref map) => (false, ext::extract_type_option(&map.value), Some(&map.key), &map.value),
	};
	let is_option = extracted_type.is_some();
	let typ = extracted_type.unwrap_or(quote!( #full_type ));
	DeclStorageTypeInfos {
		is_simple,
		full_type,
		is_option,
		typ,
		map_key,
	}
}
