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

//! Derive macro for creating XCMs with a builder pattern

use inflector::Inflector;
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
	parse_macro_input, Data, DeriveInput, Error, Expr, ExprLit, Fields, Lit, Meta, MetaNameValue,
};

pub fn derive(input: TokenStream) -> TokenStream {
	let input = parse_macro_input!(input as DeriveInput);
	let builder_impl = match &input.data {
		Data::Enum(data_enum) => generate_methods_for_enum(input.ident, data_enum),
		_ =>
			return Error::new_spanned(&input, "Expected the `Instruction` enum")
				.to_compile_error()
				.into(),
	};
	let output = quote! {
		pub struct XcmBuilder<Call>(Vec<Instruction<Call>>);
		impl<Call> Xcm<Call> {
			pub fn builder() -> XcmBuilder<Call> {
				XcmBuilder::<Call>(Vec::new())
			}
		}
		#builder_impl
	};
	output.into()
}

fn generate_methods_for_enum(name: syn::Ident, data_enum: &syn::DataEnum) -> TokenStream2 {
	let methods = data_enum.variants.iter().map(|variant| {
		let variant_name = &variant.ident;
		let method_name_string = &variant_name.to_string().to_snake_case();
		let method_name = syn::Ident::new(&method_name_string, variant_name.span());
		let docs: Vec<_> = variant
			.attrs
			.iter()
			.filter_map(|attr| match &attr.meta {
				Meta::NameValue(MetaNameValue {
					value: Expr::Lit(ExprLit { lit: Lit::Str(literal), .. }),
					..
				}) if attr.path().is_ident("doc") => Some(literal.value()),
				_ => None,
			})
			.map(|doc| syn::parse_str::<TokenStream2>(&format!("/// {}", doc)).unwrap())
			.collect();
		let method = match &variant.fields {
			Fields::Unit => {
				quote! {
					pub fn #method_name(mut self) -> Self {
						self.0.push(#name::<Call>::#variant_name);
						self
					}
				}
			},
			Fields::Unnamed(fields) => {
				let arg_names: Vec<_> = fields
					.unnamed
					.iter()
					.enumerate()
					.map(|(index, _)| format_ident!("arg{}", index))
					.collect();
				let arg_types: Vec<_> = fields.unnamed.iter().map(|field| &field.ty).collect();
				quote! {
					pub fn #method_name(mut self, #(#arg_names: #arg_types),*) -> Self {
						self.0.push(#name::<Call>::#variant_name(#(#arg_names),*));
						self
					}
				}
			},
			Fields::Named(fields) => {
				let arg_names: Vec<_> = fields.named.iter().map(|field| &field.ident).collect();
				let arg_types: Vec<_> = fields.named.iter().map(|field| &field.ty).collect();
				quote! {
					pub fn #method_name(mut self, #(#arg_names: #arg_types),*) -> Self {
						self.0.push(#name::<Call>::#variant_name { #(#arg_names),* });
						self
					}
				}
			},
		};
		quote! {
			#(#docs)*
			#method
		}
	});
	let output = quote! {
		impl<Call> XcmBuilder<Call> {
			#(#methods)*

			pub fn build(self) -> Xcm<Call> {
				Xcm(self.0)
			}
		}
	};
	output
}
