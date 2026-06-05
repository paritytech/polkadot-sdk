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

pub fn derive(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
	let input: syn::DeriveInput = match syn::parse(item) {
		Ok(input) => input,
		Err(e) => return e.into_compile_error().into(),
	};

	let syn::DeriveInput { generics, data, .. } = input;

	match data {
		syn::Data::Enum(syn::DataEnum { variants, .. }) => {
			let methods = variants.into_iter().map(|syn::Variant { ident, fields, .. }| {
				let snake_cased_ident = format_ident!("{}", ident.to_string().to_snake_case());
				let ref_fields =
					fields.into_iter().enumerate().map(|(idx, syn::Field { ident, ty, .. })| {
						let field_name = ident.unwrap_or_else(|| format_ident!("_{}", idx));
						let field_ty = match ty {
							syn::Type::Reference(r) => {
								// If the type is already a reference, do nothing
								quote::quote!(#r)
							},
							t => {
								// Otherwise, make it a reference
								quote::quote!(&#t)
							},
						};

						quote::quote!(#field_name: #field_ty,)
					});
				quote::quote!(fn #snake_cased_ident( #(#ref_fields)* ) -> Weight;)
			});

			let res = quote::quote! {
				pub trait XcmWeightInfo #generics {
					#(#methods)*
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

pub fn derive_impl(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
	let input: syn::DeriveInput = match syn::parse(item) {
		Ok(input) => input,
		Err(e) => return e.into_compile_error().into(),
	};

	let syn::DeriveInput { mut generics, data, ident, .. } = input;
	let enum_ty_generics = {
		let (_, ty_generics, _) = generics.split_for_impl();
		quote::quote!(#ty_generics)
	};
	generics.params.push(syn::parse_quote!(W));
	generics
		.make_where_clause()
		.predicates
		.push(syn::parse_quote!(W: XcmWeightInfo #enum_ty_generics));
	let (impl_generics, _, where_clause) = generics.split_for_impl();

	match data {
		syn::Data::Enum(syn::DataEnum { variants, .. }) => {
			let match_arms = variants.into_iter().map(|syn::Variant { ident: variant_ident, fields, .. }| {
				let snake_cased_ident =
					format_ident!("{}", variant_ident.to_string().to_snake_case());
				match fields {
					syn::Fields::Unit => {
						quote::quote!(#ident::#variant_ident => W::#snake_cased_ident(),)
					},
					syn::Fields::Unnamed(fields_unnamed) => {
						let field_names = (0..fields_unnamed.unnamed.len())
							.map(|idx| format_ident!("_{}", idx))
							.collect::<Vec<_>>();
						quote::quote!(
							#ident::#variant_ident( #(#field_names),* ) =>
								W::#snake_cased_ident( #(#field_names),* ),
						)
					},
					syn::Fields::Named(fields_named) => {
						let field_names = fields_named
							.named
							.into_iter()
							.map(|field| field.ident.expect("named fields always have idents"))
							.collect::<Vec<_>>();
						quote::quote!(
							#ident::#variant_ident { #(#field_names),* } =>
								W::#snake_cased_ident( #(#field_names),* ),
						)
					},
				}
			});

			let res = quote::quote! {
				impl #impl_generics GetWeight<W> for #ident #enum_ty_generics
				#where_clause {
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
