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

//! Code for the `#[dynamic_params]`, `#[dynamic_pallet_params]` and
//! `#[dynamic_aggregated_params_internal]` macros.

use frame_support_procedural_tools::generate_access_from_frame_or_crate;
use inflector::Inflector;
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, ToTokens};
use syn::{parse2, spanned::Spanned, visit_mut, visit_mut::VisitMut, Result, Token};

/// Parse and expand a `#[dynamic_params(..)]` module.
pub fn dynamic_params(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
	DynamicParamModAttr::parse(attr, item).map(ToTokens::into_token_stream)
}

/// Parse and expand `#[dynamic_pallet_params(..)]` attribute.
pub fn dynamic_pallet_params(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
	DynamicPalletParamAttr::parse(attr, item).map(ToTokens::into_token_stream)
}

/// Parse and expand `#[dynamic_aggregated_params_internal]` attribute.
pub fn dynamic_aggregated_params_internal(
	_attr: TokenStream,
	item: TokenStream,
) -> Result<TokenStream> {
	parse2::<DynamicParamAggregatedEnum>(item).map(ToTokens::into_token_stream)
}

/// A top `#[dynamic_params(..)]` attribute together with a mod.
#[derive(derive_syn_parse::Parse)]
pub struct DynamicParamModAttr {
	params_mod: syn::ItemMod,
	meta: DynamicParamModAttrMeta,
}

/// The inner meta of a `#[dynamic_params(..)]` attribute.
#[derive(derive_syn_parse::Parse)]
pub struct DynamicParamModAttrMeta {
	name: syn::Ident,
	_comma: Option<Token![,]>,
	#[parse_if(_comma.is_some())]
	params_pallet: Option<syn::Type>,
}

impl DynamicParamModAttr {
	pub fn parse(attr: TokenStream, item: TokenStream) -> Result<Self> {
		let params_mod = parse2(item)?;
		let meta = parse2(attr)?;
		Ok(Self { params_mod, meta })
	}

	pub fn inner_mods(&self) -> Vec<syn::ItemMod> {
		self.params_mod.content.as_ref().map_or(Vec::new(), |(_, items)| {
			items
				.iter()
				.filter_map(|i| match i {
					syn::Item::Mod(m) => Some(m),
					_ => None,
				})
				.cloned()
				.collect()
		})
	}
}

impl ToTokens for DynamicParamModAttr {
	fn to_tokens(&self, tokens: &mut TokenStream) {
		let scrate = match crate_access() {
			Ok(path) => path,
			Err(err) => return tokens.extend(err),
		};
		let (mut params_mod, name) = (self.params_mod.clone(), &self.meta.name);
		let dynam_params_ident = &params_mod.ident;

		let mut quoted_enum = quote! {};
		for m in self.inner_mods() {
			let aggregate_name =
				syn::Ident::new(&m.ident.to_string().to_pascal_case(), m.ident.span());
			let mod_name = &m.ident;

			let mut attrs = m.attrs.clone();
			attrs.retain(|attr| !attr.path().is_ident("dynamic_pallet_params"));
			if let Err(err) = ensure_codec_index(&attrs, m.span()) {
				tokens.extend(err.into_compile_error());
				return
			}

			quoted_enum.extend(quote! {
				#(#attrs)*
				#aggregate_name(#dynam_params_ident::#mod_name::Parameters),
			});
		}

		// Inject the outer args into the inner `#[dynamic_pallet_params(..)]` attribute.
		if let Some(params_pallet) = &self.meta.params_pallet {
			MacroInjectArgs { runtime_params: name.clone(), params_pallet: params_pallet.clone() }
				.visit_item_mod_mut(&mut params_mod);
		}

		tokens.extend(quote! {
			#params_mod

			#[#scrate::dynamic_params::dynamic_aggregated_params_internal]
			pub enum #name {
				#quoted_enum
			}
		});
	}
}

/// Ensure there is a `#[codec(index = ..)]` attribute.
fn ensure_codec_index(attrs: &Vec<syn::Attribute>, span: Span) -> Result<()> {
	let mut found = false;

	for attr in attrs.iter() {
		if attr.path().is_ident("codec") {
			let meta: syn::ExprAssign = attr.parse_args()?;
			if meta.left.to_token_stream().to_string() == "index" {
				found = true;
				break
			}
		}
	}

	if !found {
		Err(syn::Error::new(span, "Missing explicit `#[codec(index = ..)]` attribute"))
	} else {
		Ok(())
	}
}

/// Used to inject arguments into the inner `#[dynamic_pallet_params(..)]` attribute.
///
/// This allows the outer `#[dynamic_params(..)]` attribute to specify some arguments that don't
/// need to be repeated every time.
struct MacroInjectArgs {
	runtime_params: syn::Ident,
	params_pallet: syn::Type,
}
impl VisitMut for MacroInjectArgs {
	fn visit_item_mod_mut(&mut self, item: &mut syn::ItemMod) {
		// Check if the mod has a `#[dynamic_pallet_params(..)]` attribute.
		let attr = item.attrs.iter_mut().find(|attr| attr.path().is_ident("dynamic_pallet_params"));

		if let Some(attr) = attr {
			match &attr.meta {
				syn::Meta::Path(path) =>
					assert_eq!(path.to_token_stream().to_string(), "dynamic_pallet_params"),
				_ => (),
			}

			let runtime_params = &self.runtime_params;
			let params_pallet = &self.params_pallet;

			attr.meta = syn::parse2::<syn::Meta>(quote! {
				dynamic_pallet_params(#runtime_params, #params_pallet)
			})
			.unwrap()
			.into();
		}

		visit_mut::visit_item_mod_mut(self, item);
	}
}
/// The helper attribute of a `#[dynamic_pallet_params(runtime_params, params_pallet)]`
/// attribute.
#[derive(derive_syn_parse::Parse)]
pub struct DynamicPalletParamAttr {
	inner_mod: syn::ItemMod,
	meta: DynamicPalletParamAttrMeta,
}

/// The inner meta of a `#[dynamic_pallet_params(..)]` attribute.
#[derive(derive_syn_parse::Parse)]
pub struct DynamicPalletParamAttrMeta {
	runtime_params: syn::Ident,
	_comma: Token![,],
	parameter_pallet: syn::Type,
}

impl DynamicPalletParamAttr {
	pub fn parse(attr: TokenStream, item: TokenStream) -> Result<Self> {
		Ok(Self { inner_mod: parse2(item)?, meta: parse2(attr)? })
	}

	pub fn statics(&self) -> Vec<syn::ItemStatic> {
		self.inner_mod.content.as_ref().map_or(Vec::new(), |(_, items)| {
			items
				.iter()
				.filter_map(|i| match i {
					syn::Item::Static(s) => Some(s),
					_ => None,
				})
				.cloned()
				.collect()
		})
	}
}

impl ToTokens for DynamicPalletParamAttr {
	fn to_tokens(&self, tokens: &mut TokenStream) {
		let scrate = match crate_access() {
			Ok(path) => path,
			Err(err) => return tokens.extend(err),
		};
		let (params_mod, parameter_pallet, runtime_params) =
			(&self.inner_mod, &self.meta.parameter_pallet, &self.meta.runtime_params);

		let aggregate_name = syn::Ident::new(
			&params_mod.ident.to_string().to_pascal_case(),
			params_mod.ident.span(),
		);
		let (mod_name, vis) = (&params_mod.ident, &params_mod.vis);
		let statics = self.statics();

		let (mut key_names, mut key_values, mut defaults, mut attrs, mut value_types): (
			Vec<_>,
			Vec<_>,
			Vec<_>,
			Vec<_>,
			Vec<_>,
		) = Default::default();

		for s in statics.iter() {
			if let Err(err) = ensure_codec_index(&s.attrs, s.span()) {
				tokens.extend(err.into_compile_error());
				return
			}

			key_names.push(&s.ident);
			key_values.push(format_ident!("{}Value", &s.ident));
			defaults.push(&s.expr);
			attrs.push(&s.attrs);
			value_types.push(&s.ty);
		}

		let key_ident = syn::Ident::new("ParametersKey", params_mod.ident.span());
		let value_ident = syn::Ident::new("ParametersValue", params_mod.ident.span());
		let runtime_key_ident = format_ident!("{}Key", runtime_params);
		let runtime_value_ident = format_ident!("{}Value", runtime_params);

		tokens.extend(quote! {
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
				#vis enum Parameters {
					#(
						#(#attrs)*
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
						#(#attrs)*
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
						#(#attrs)*
						#key_names(#value_types),
					)*
				}

				impl #scrate::traits::dynamic_params::AggregatedKeyValue for Parameters {
					type Key = #key_ident;
					type Value = #value_ident;

					fn into_parts(self) -> (Self::Key, Option<Self::Value>) {
						match self {
							#(
								Parameters::#key_names(key, value) => {
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
									#scrate::storage::StorageMap<#runtime_key_ident, #runtime_value_ident>
								>::get(#runtime_key_ident::#aggregate_name(#key_ident::#key_names(#key_names)))
							{
								Some(#runtime_value_ident::#aggregate_name(
									#value_ident::#key_names(inner))) => inner,
								Some(_) => {
									#scrate::defensive!("Unexpected value type at key - returning default");
									#defaults
								},
								None => #defaults,
							}
						}
					}

					impl #scrate::traits::dynamic_params::Key for #key_names {
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

					impl From<(#key_names, #value_types)> for Parameters {
						fn from((key, value): (#key_names, #value_types)) -> Self {
							Parameters::#key_names(key, Some(value))
						}
					}

					impl From<#key_names> for Parameters {
						fn from(key: #key_names) -> Self {
							Parameters::#key_names(key, None)
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
		});
	}
}

#[derive(derive_syn_parse::Parse)]
pub struct DynamicParamAggregatedEnum {
	aggregated_enum: syn::ItemEnum,
}

impl ToTokens for DynamicParamAggregatedEnum {
	fn to_tokens(&self, tokens: &mut TokenStream) {
		let scrate = match crate_access() {
			Ok(path) => path,
			Err(err) => return tokens.extend(err),
		};
		let params_enum = &self.aggregated_enum;
		let (name, vis) = (&params_enum.ident, &params_enum.vis);

		let (mut indices, mut param_names, mut param_types): (Vec<_>, Vec<_>, Vec<_>) =
			Default::default();
		let mut attributes = Vec::new();
		for (i, variant) in params_enum.variants.iter().enumerate() {
			indices.push(i);
			param_names.push(&variant.ident);
			attributes.push(&variant.attrs);

			param_types.push(match &variant.fields {
				syn::Fields::Unnamed(fields) if fields.unnamed.len() == 1 => &fields.unnamed[0].ty,
				_ => {
					*tokens = quote! { compile_error!("Only unnamed enum variants with one inner item are supported") };
					return
				},
			});
		}

		let params_key_ident = format_ident!("{}Key", params_enum.ident);
		let params_value_ident = format_ident!("{}Value", params_enum.ident);

		tokens.extend(quote! {
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
					//#[codec(index = #indices)]
					#(#attributes)*
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
					#(#attributes)*
					#param_names(<#param_types as #scrate::traits::dynamic_params::AggregatedKeyValue>::Key),
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
					#(#attributes)*
					#param_names(<#param_types as #scrate::traits::dynamic_params::AggregatedKeyValue>::Value),
				)*
			}

			impl #scrate::traits::dynamic_params::AggregatedKeyValue for #name {
				type Key = #params_key_ident;
				type Value = #params_value_ident;

				fn into_parts(self) -> (Self::Key, Option<Self::Value>) {
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
				impl ::core::convert::From<<#param_types as #scrate::traits::dynamic_params::AggregatedKeyValue>::Key> for #params_key_ident {
					fn from(key: <#param_types as #scrate::traits::dynamic_params::AggregatedKeyValue>::Key) -> Self {
						#params_key_ident::#param_names(key)
					}
				}

				impl ::core::convert::TryFrom<#params_value_ident> for <#param_types as #scrate::traits::dynamic_params::AggregatedKeyValue>::Value {
					type Error = ();

					fn try_from(value: #params_value_ident) -> Result<Self, Self::Error> {
						match value {
							#params_value_ident::#param_names(value) => Ok(value),
							_ => Err(()),
						}
					}
				}
			)*
		});
	}
}

/// Get access to the current crate and convert the error to a compile error.
fn crate_access() -> core::result::Result<syn::Path, TokenStream> {
	generate_access_from_frame_or_crate("frame-support").map_err(|e| e.to_compile_error())
}
