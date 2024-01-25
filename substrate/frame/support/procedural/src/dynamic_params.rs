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

use frame_support_procedural_tools::generate_access_from_frame_or_crate;
use inflector::Inflector;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse2, token, Result, Token};

#[derive(derive_syn_parse::Parse)]
pub struct DynamicPalletParamAttr {
	parameter_pallet: syn::Type,
	_common: syn::token::Comma,
	parameter_name: syn::Ident,
}

pub fn dynamic_pallet_params(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
	let scrate = generate_access_from_frame_or_crate("frame-support")?;
	let params_mod = parse2::<syn::ItemMod>(item)?;

	let DynamicPalletParamAttr { parameter_pallet, parameter_name, .. } =
		syn::parse2::<DynamicPalletParamAttr>(attr.clone())?;
	let mod_name = params_mod.clone().ident;
	let name = parameter_name.clone();
	let aggregate_name = syn::Ident::new(
		params_mod.ident.to_string().to_class_case().as_str(),
		params_mod.ident.span(),
	);
	let vis = params_mod.clone().vis;

	let (_, items) = params_mod.content.clone().unwrap();
	let key_names = items
		.iter()
		.filter_map(|item| match item {
			syn::Item::Static(static_item) => Some(static_item.ident.clone()),
			_ => None,
		})
		.collect::<Vec<_>>();

	let key_values = items
		.iter()
		.filter_map(|item| match item {
			syn::Item::Static(static_item) => Some(syn::Ident::new(
				&format!("{}Value", static_item.ident.clone()),
				static_item.ident.span(),
			)),
			_ => None,
		})
		.collect::<Vec<_>>();

	let defaults = items
		.iter()
		.filter_map(|item| match item {
			syn::Item::Static(static_item) => Some(static_item.expr.clone()),
			_ => None,
		})
		.collect::<Vec<_>>();

	let attrs = items
		.iter()
		.filter_map(|item| match item {
			syn::Item::Static(static_item) => Some(static_item.attrs.first().clone()),
			_ => None,
		})
		.collect::<Vec<_>>();

	let value_types = items
		.iter()
		.filter_map(|item| match item {
			syn::Item::Static(static_item) => Some(static_item.ty.clone()),
			_ => None,
		})
		.collect::<Vec<_>>();

	let key_ident = syn::Ident::new(&format!("{}Key", name), params_mod.ident.span());
	let value_ident = syn::Ident::new(&format!("{}Value", name), params_mod.ident.span());

	let res = quote! {
		pub mod #mod_name {
			use super::*;

			#[doc(hidden)]
			#[derive(
				Clone,
				PartialEq,
				Eq,
				#scrate::__private::codec::Encode,
				#scrate::__private::codec::Decode,
				#scrate::__private::codec::MaxEncodedLen,
				#scrate::__private::RuntimeDebug,
				#scrate::__private::scale_info::TypeInfo
			)]
			#vis enum #name {
				#(
					#attrs
					#key_names(#key_names, Option<#value_types>),
				)*
			}

			#[doc(hidden)]
			#[derive(
				Clone,
				PartialEq,
				Eq,
				#scrate::__private::codec::Encode,
				#scrate::__private::codec::Decode,
				#scrate::__private::codec::MaxEncodedLen,
				#scrate::__private::RuntimeDebug,
				#scrate::__private::scale_info::TypeInfo
			)]
			#vis enum #key_ident {
				#(
					#attrs
					#key_names(#key_names),
				)*
			}

			#[doc(hidden)]
			#[derive(
				Clone,
				PartialEq,
				Eq,
				#scrate::__private::codec::Encode,
				#scrate::__private::codec::Decode,
				#scrate::__private::codec::MaxEncodedLen,
				#scrate::__private::RuntimeDebug,
				#scrate::__private::scale_info::TypeInfo
			)]
			#vis enum #value_ident {
				#(
					#attrs
					#key_names(#value_types),
				)*
			}

			impl #scrate::traits::AggregratedKeyValue for #name {
				type AggregratedKey = #key_ident;
				type AggregratedValue = #value_ident;

				fn into_parts(self) -> (Self::AggregratedKey, Option<Self::AggregratedValue>) {
					match self {
						#(
							#name::#key_names(key, value) => {
								(#key_ident::#key_names(key), value.map(#value_ident::#key_names))
							},
						)*
					}
				}
			}

			#(
				#[doc(hidden)]
				#[derive(
					Clone,
					PartialEq,
					Eq,
					#scrate::__private::codec::Encode,
					#scrate::__private::codec::Decode,
					#scrate::__private::codec::MaxEncodedLen,
					#scrate::__private::RuntimeDebug,
					#scrate::__private::scale_info::TypeInfo
				)]
				#vis struct #key_names;

				impl #scrate::__private::Get<#value_types> for #key_names {
					fn get() -> #value_types {
						match
							<#parameter_pallet as
								#scrate::storage::StorageMap<RuntimeParametersKey, RuntimeParametersValue>
							>::get(RuntimeParametersKey::#aggregate_name(#key_ident::#key_names(#key_names)))
						{
							Some(RuntimeParametersValue::#aggregate_name(
								#value_ident::#key_names(inner))) => inner,
							Some(_) => {
								#scrate::defensive!("Unexpected value type at key - returning default");
								#defaults
							},
							None => #defaults,
						}
					}
				}

				impl #scrate::traits::Key for #key_names {
					type Value = #value_types;
					type WrappedValue = #key_values;
				}

				impl From<#key_names> for #key_ident {
					fn from(key: #key_names) -> Self {
						#key_ident::#key_names(key)
					}
				}

				impl TryFrom<#key_ident> for #key_names {
					type Error = ();

					fn try_from(key: #key_ident) -> Result<Self, Self::Error> {
						match key {
							#key_ident::#key_names(key) => Ok(key),
							_ => Err(()),
						}
					}
				}

				#[doc(hidden)]
				#[derive(
					Clone,
					PartialEq,
					Eq,
					#scrate::sp_runtime::RuntimeDebug,
				)]
				#vis struct #key_values(pub #value_types);

				impl From<#key_values> for #value_ident {
					fn from(value: #key_values) -> Self {
						#value_ident::#key_names(value.0)
					}
				}

				impl From<(#key_names, #value_types)> for #name {
					fn from((key, value): (#key_names, #value_types)) -> Self {
						#name::#key_names(key, Some(value))
					}
				}

				impl From<#key_names> for #name {
					fn from(key: #key_names) -> Self {
						#name::#key_names(key, None)
					}
				}

				impl TryFrom<#value_ident> for #key_values {
					type Error = ();

					fn try_from(value: #value_ident) -> Result<Self, Self::Error> {
						match value {
							#value_ident::#key_names(value) => Ok(#key_values(value)),
							_ => Err(()),
						}
					}
				}

				impl From<#key_values> for #value_types {
					fn from(value: #key_values) -> Self {
						value.0
					}
				}
			)*
		}
	};

	// use proc_utils::*;
	// res.pretty_print();

	Ok(res)
}

pub fn dynamic_aggregated_params(_attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
	let scrate = generate_access_from_frame_or_crate("frame-support")?;
	let params_enum = parse2::<syn::ItemEnum>(item)?;

	let name = params_enum.clone().ident;
	let vis = params_enum.clone().vis;

	let param_names = params_enum
		.variants
		.iter()
		.map(|variant| variant.ident.clone())
		.collect::<Vec<_>>();

	let param_types = params_enum
		.variants
		.iter()
		.map(|variant| match &variant.fields {
			syn::Fields::Unnamed(fields) => {
				assert_eq!(fields.unnamed.len(), 1);
				fields.unnamed.iter().next().unwrap().ty.clone()
			},
			_ => panic!("Only unnamed fields are supported"),
		})
		.collect::<Vec<_>>();

	let indices = params_enum.variants.iter().enumerate().map(|(i, _)| i).collect::<Vec<_>>();

	let params_key_ident: proc_macro2::Ident =
		syn::Ident::new(&format!("{}Key", params_enum.ident), params_enum.ident.span());
	let params_value_ident =
		syn::Ident::new(&format!("{}Value", params_enum.ident), params_enum.ident.span());

	let res = quote! {
		#[doc(hidden)]
		#[derive(
			Clone,
			PartialEq,
			Eq,
			#scrate::__private::codec::Encode,
			#scrate::__private::codec::Decode,
			#scrate::__private::codec::MaxEncodedLen,
			#scrate::sp_runtime::RuntimeDebug,
			#scrate::__private::scale_info::TypeInfo
		)]
		#vis enum #name {
			#(
				#[codec(index = #indices)]
				#param_names(#param_types),
			)*
		}

		#[doc(hidden)]
		#[derive(
			Clone,
			PartialEq,
			Eq,
			#scrate::__private::codec::Encode,
			#scrate::__private::codec::Decode,
			#scrate::__private::codec::MaxEncodedLen,
			#scrate::sp_runtime::RuntimeDebug,
			#scrate::__private::scale_info::TypeInfo
		)]
		#vis enum #params_key_ident {
			#(
				#[codec(index = #indices)]
				#param_names(<#param_types as #scrate::traits::AggregratedKeyValue>::AggregratedKey),
			)*
		}

		#[doc(hidden)]
		#[derive(
			Clone,
			PartialEq,
			Eq,
			#scrate::__private::codec::Encode,
			#scrate::__private::codec::Decode,
			#scrate::__private::codec::MaxEncodedLen,
			#scrate::sp_runtime::RuntimeDebug,
			#scrate::__private::scale_info::TypeInfo
		)]
		#vis enum #params_value_ident {
			#(
				#[codec(index = #indices)]
				#param_names(<#param_types as #scrate::traits::AggregratedKeyValue>::AggregratedValue),
			)*
		}

		impl #scrate::traits::AggregratedKeyValue for #name {
			type AggregratedKey = #params_key_ident;
			type AggregratedValue = #params_value_ident;

			fn into_parts(self) -> (Self::AggregratedKey, Option<Self::AggregratedValue>) {
				match self {
					#(
						#name::#param_names(parameter) => {
							let (key, value) = parameter.into_parts();
							(#params_key_ident::#param_names(key), value.map(#params_value_ident::#param_names))
						},
					)*
				}
			}
		}

		#(
			impl ::core::convert::From<<#param_types as #scrate::traits::AggregratedKeyValue>::AggregratedKey> for #params_key_ident {
				fn from(key: <#param_types as #scrate::traits::AggregratedKeyValue>::AggregratedKey) -> Self {
					#params_key_ident::#param_names(key)
				}
			}

			impl ::core::convert::TryFrom<#params_value_ident> for <#param_types as #scrate::traits::AggregratedKeyValue>::AggregratedValue {
				type Error = ();

				fn try_from(value: #params_value_ident) -> Result<Self, Self::Error> {
					match value {
						#params_value_ident::#param_names(value) => Ok(value),
						_ => Err(()),
					}
				}
			}
		)*
	};

	Ok(res)
}

mod keyword {
	syn::custom_keyword!(dynamic_pallet_params);
}

#[derive(derive_syn_parse::Parse)]
pub struct DynamicParamModAttrMeta {
	_keyword: keyword::dynamic_pallet_params,
	#[paren]
	_paren: token::Paren,
	#[inside(_paren)]
	pallet_param_attr: DynamicPalletParamAttr,
}

#[derive(derive_syn_parse::Parse)]
pub struct DynamicParamModAttr {
	_pound: Token![#],
	#[bracket]
	_bracket: token::Bracket,
	#[inside(_bracket)]
	meta: DynamicParamModAttrMeta,
}

pub fn dynamic_params(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
	let scrate = generate_access_from_frame_or_crate("frame-support")?;
	let params_mod = parse2::<syn::ItemMod>(item.clone())?;
	let name = parse2::<syn::Ident>(attr)?;

	let (_, items) = params_mod.content.clone().unwrap();
	let aggregate_names = items
		.iter()
		.filter_map(|item| match item {
			syn::Item::Mod(params_mod) => Some(syn::Ident::new(
				params_mod.ident.to_string().to_class_case().as_str(),
				name.span(),
			)),
			_ => None,
		})
		.collect::<Vec<_>>();
	let mod_names = items
		.iter()
		.filter_map(|item| match item {
			syn::Item::Mod(params_mod) => Some(params_mod.ident.clone()),
			_ => None,
		})
		.collect::<Vec<_>>();
	let parameter_names = items
		.iter()
		.filter_map(|item| match item {
			syn::Item::Mod(params_mod) => {
				let attr = params_mod.attrs.first();
				let Ok(DynamicParamModAttr { meta, .. }) =
					syn::parse2::<DynamicParamModAttr>(quote! { #attr })
				else {
					return None;
				};
				Some(meta.pallet_param_attr.parameter_name)
			},
			_ => None,
		})
		.collect::<Vec<_>>();

	let dynam_params_ident = params_mod.ident;
	let res = quote! {
		#item

		#[#scrate::dynamic_params::dynamic_aggregated_params]
		pub enum #name {
			#(
				#aggregate_names(#dynam_params_ident::#mod_names::#parameter_names),
			)*
		}
	};

	Ok(res)
}

#[test]
fn test_mod_attr_parser() {
	let attr = quote! {
		#[dynamic_pallet_params(pallet_parameters::Parameters::<Test>, Basic)]
	};
	let attr = syn::parse2::<DynamicParamModAttr>(attr).unwrap();
	assert_eq!(attr.meta.pallet_param_attr.parameter_name.to_string(), "Basic");
}
