// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use inflector::Inflector;
use quote::format_ident;

/// Name of the generic parameter that the generated `GetWeight` impl uses for the weight provider.
const WEIGHT_INFO_PROVIDER: &str = "W";

pub fn derive(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
	let input: syn::DeriveInput = match syn::parse(item) {
		Ok(input) => input,
		Err(e) => return e.into_compile_error().into(),
	};

	let syn::DeriveInput { generics, data, ident, .. } = input;

	let reserved_generic = generics.params.iter().find(|param| match param {
		syn::GenericParam::Type(ty) => ty.ident == WEIGHT_INFO_PROVIDER,
		syn::GenericParam::Const(c) => c.ident == WEIGHT_INFO_PROVIDER,
		syn::GenericParam::Lifetime(_) => false,
	});
	if let Some(param) = reserved_generic {
		return syn::Error::new_spanned(
			param,
			format!(
				"the generic parameter name `{WEIGHT_INFO_PROVIDER}` is reserved by \
				 `XcmWeightInfoTrait` for the weight provider; rename this parameter"
			),
		)
		.into_compile_error()
		.into();
	}

	let weight_info_provider_ident = format_ident!("{}", WEIGHT_INFO_PROVIDER);
	let enum_ty_generics = {
		let (_, ty_generics, _) = generics.split_for_impl();
		quote::quote!(#ty_generics)
	};
	let (impl_generics, where_clause) = {
		let mut impl_source_generics = generics.clone();
		impl_source_generics.params.push(syn::parse_quote!(#weight_info_provider_ident));
		impl_source_generics.make_where_clause().predicates.push(syn::parse_quote!(
			#weight_info_provider_ident: XcmWeightInfo #enum_ty_generics
		));
		let (impl_generics, _, where_clause) = impl_source_generics.split_for_impl();
		(quote::quote!(#impl_generics), quote::quote!(#where_clause))
	};

	match data {
		syn::Data::Enum(syn::DataEnum { variants, .. }) => {
			let (methods, match_arms): (Vec<_>, Vec<_>) = variants
				.into_iter()
				.map(|syn::Variant { ident: variant_ident, fields, .. }| {
					let snake_cased_ident =
						format_ident!("{}", variant_ident.to_string().to_snake_case());

					// Field binding names, derived once and reused by both the trait method
					// signature and the match arm. Named fields keep their name; the unnamed
					// fields of a tuple variant become `_0`, `_1`, ... .
					let field_names = fields
						.iter()
						.enumerate()
						.map(|(index, field)| {
							field.ident.clone().unwrap_or_else(|| format_ident!("_{}", index))
						})
						.collect::<Vec<_>>();

					// Trait method: one `name: &Type` parameter per field.
					let params = fields.iter().zip(&field_names).map(|(field, name)| {
						let field_ty = match &field.ty {
							// If the type is already a reference, do nothing
							syn::Type::Reference(r) => quote::quote!(#r),
							// Otherwise, make it a reference
							ty => quote::quote!(&#ty),
						};
						quote::quote!(#name: #field_ty,)
					});
					let method = quote::quote!(fn #snake_cased_ident( #(#params)* ) -> Weight;);

					// Match arm: destructure the variant and forward its fields to the method.
					let match_arm = match fields {
						syn::Fields::Unit => quote::quote!(
							#ident::#variant_ident =>
								#weight_info_provider_ident::#snake_cased_ident(),
						),
						syn::Fields::Unnamed(_) => quote::quote!(
							#ident::#variant_ident( #(#field_names),* ) =>
								#weight_info_provider_ident::#snake_cased_ident( #(#field_names),* ),
						),
						syn::Fields::Named(_) => quote::quote!(
							#ident::#variant_ident { #(#field_names),* } =>
								#weight_info_provider_ident::#snake_cased_ident( #(#field_names),* ),
						),
					};

					(method, match_arm)
				})
				.unzip();

			// `match self {}` is rejected for references even when the enum is empty.
			let body = if match_arms.is_empty() {
				quote::quote!(match *self {})
			} else {
				quote::quote!(match self { #(#match_arms)* })
			};

			let res = quote::quote! {
				pub trait XcmWeightInfo #generics {
					#(#methods)*
				}

				impl #impl_generics GetWeight<#weight_info_provider_ident> for #ident #enum_ty_generics #where_clause {
					fn weight(&self) -> Weight {
						#body
					}
				}
			};
			res.into()
		},
		syn::Data::Struct(syn::DataStruct { struct_token, .. }) => {
			let msg = "structs are not supported by 'derive(XcmWeightInfo)'";
			syn::Error::new(struct_token.span, msg).into_compile_error().into()
		},
		syn::Data::Union(syn::DataUnion { union_token, .. }) => {
			let msg = "unions are not supported by 'derive(XcmWeightInfo)'";
			syn::Error::new(union_token.span, msg).into_compile_error().into()
		},
	}
}
