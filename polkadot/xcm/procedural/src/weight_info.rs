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

fn generic_ident_is_taken(generics: &syn::Generics, ident: &syn::Ident) -> bool {
	generics.params.iter().any(|param| match param {
		syn::GenericParam::Type(param) => param.ident == *ident,
		syn::GenericParam::Const(param) => param.ident == *ident,
		syn::GenericParam::Lifetime(_) => false,
	})
}

fn weight_info_provider_ident(generics: &syn::Generics) -> syn::Ident {
	let mut index = 0;
	loop {
		let ident = if index == 0 { format_ident!("W") } else { format_ident!("W{index}") };
		if !generic_ident_is_taken(generics, &ident) {
			return ident;
		}
		index += 1;
	}
}

pub fn derive(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
	let input: syn::DeriveInput = match syn::parse(item) {
		Ok(input) => input,
		Err(e) => return e.into_compile_error().into(),
	};

	let syn::DeriveInput { ident, generics, data, .. } = input;

	match data {
		syn::Data::Enum(syn::DataEnum { variants, .. }) => {
			let weight_info_provider = weight_info_provider_ident(&generics);
			let (_, ty_generics, _) = generics.split_for_impl();

			let mut implementation_generics = generics.clone();
			implementation_generics.params.push(syn::parse_quote!(#weight_info_provider));
			implementation_generics
				.make_where_clause()
				.predicates
				.push(syn::parse_quote!(#weight_info_provider: XcmWeightInfo #ty_generics));
			let (impl_generics, _, implementation_where_clause) =
				implementation_generics.split_for_impl();

			let (methods, match_arms): (Vec<_>, Vec<_>) = variants
				.into_iter()
				.map(|syn::Variant { ident: variant_ident, fields, .. }| {
					let method_ident =
						format_ident!("{}", variant_ident.to_string().to_snake_case());
					let field_idents = fields
						.iter()
						.enumerate()
						.map(|(index, field)| {
							field.ident.clone().unwrap_or_else(|| format_ident!("_{index}"))
						})
						.collect::<Vec<_>>();
					let method_parameters =
						fields.iter().zip(&field_idents).map(|(field, ident)| {
							let ty = match &field.ty {
								syn::Type::Reference(reference) => quote::quote!(#reference),
								ty => quote::quote!(&#ty),
							};
							quote::quote!(#ident: #ty,)
						});
					let method =
						quote::quote!(fn #method_ident( #(#method_parameters)* ) -> Weight;);
					let match_arm = match fields {
						syn::Fields::Unit => quote::quote!(
							#ident::#variant_ident =>
								#weight_info_provider::#method_ident(),
						),
						syn::Fields::Unnamed(_) => quote::quote!(
							#ident::#variant_ident( #(#field_idents),* ) =>
								#weight_info_provider::#method_ident( #(#field_idents),* ),
						),
						syn::Fields::Named(_) => quote::quote!(
							#ident::#variant_ident { #(#field_idents),* } =>
								#weight_info_provider::#method_ident( #(#field_idents),* ),
						),
					};

					(method, match_arm)
				})
				.unzip();

			let res = quote::quote! {
				pub trait XcmWeightInfo #generics {
					#(#methods)*
				}

				impl #impl_generics GetWeight<#weight_info_provider> for #ident #ty_generics
				#implementation_where_clause
				{
					fn weight(&self) -> Weight {
						match self {
							#(#match_arms)*
						}
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
