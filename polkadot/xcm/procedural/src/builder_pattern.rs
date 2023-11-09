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
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
	Data, DeriveInput, Error, Expr, ExprLit, Fields, Lit, Meta, MetaNameValue,
	Result, Variant, Ident, DataEnum, parse_quote,
};

pub fn derive(input: DeriveInput) -> Result<TokenStream2> {
	let data_enum = match &input.data {
		Data::Enum(data_enum) => data_enum,
		_ => return Err(Error::new_spanned(&input, "Expected the `Instruction` enum"))
	};
	let builder_raw_impl = generate_builder_raw_impl(&input.ident, data_enum);
	let builder_impl = generate_builder_impl(&input.ident, data_enum)?;
	let builder_unpaid_impl = generate_builder_unpaid_impl(&input.ident, data_enum)?;
	let output = quote! {
		/// Type used to build XCM programs that require fee payment
		pub struct XcmBuilder<Call>(pub(crate) Vec<Instruction<Call>>);
		/// Type used to build XCM programs that require explicitly stating no fees need to be paid
		pub struct XcmBuilderUnpaid<Call>(pub(crate) Vec<Instruction<Call>>);
		/// Type used to build arbitrary XCM, without restrictions.
		/// Should only be used when you know what you're doing, for experimenting or for learning/teaching purposes.
		pub struct XcmBuilderRaw<Call>(pub(crate) Vec<Instruction<Call>>);
		impl<Call> Xcm<Call> {
			pub fn builder() -> XcmBuilder<Call> {
				XcmBuilder::<Call>(Vec::new())
			}
			pub fn builder_unpaid() -> XcmBuilderUnpaid<Call> {
				XcmBuilderUnpaid::<Call>(Vec::new())
			}
			pub fn builder_unsafe() -> XcmBuilderRaw<Call> {
				XcmBuilderRaw::<Call>(Vec::new())
			}
		}
		#builder_impl
		#builder_unpaid_impl
		#builder_raw_impl
	};
	Ok(output)
}

fn generate_builder_raw_impl(name: &Ident, data_enum: &DataEnum) -> TokenStream2 {
	let methods = data_enum.variants.iter().map(|variant| {
		let variant_name = &variant.ident;
		let method_name_string = &variant_name.to_string().to_snake_case();
		let method_name = syn::Ident::new(&method_name_string, variant_name.span());
		let docs = get_doc_comments(&variant);
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
		impl<Call> XcmBuilderRaw<Call> {
			#(#methods)*

			/// Create an instance of this type with some pre-filled instructions.
			/// Useful to create additional builders with some restrictions beforehand.
			pub(crate) fn with_instructions(instructions: Vec<#name<Call>>) -> Self {
				Self(instructions)
			}

			pub fn build(self) -> Xcm<Call> {
				Xcm(self.0)
			}
		}
	};
	output
}

/// All instructions that load the holding register
const LOAD_HOLDING_INSTRUCTIONS: &[&str] = &[
	"WithdrawAsset",
	"ClaimAsset",
	"ReserveAssetDeposited",
	"ReceiveTeleportedAsset",
];

fn generate_builder_impl(name: &Ident, data_enum: &DataEnum) -> Result<TokenStream2> {
	let loaded_holding_builder_ident: Ident = parse_quote!(XcmLoadedHoldingBuilder);
	// We first require an instruction that load the holding register
	let load_holding_methods = data_enum.variants.iter()
		.filter(|variant| LOAD_HOLDING_INSTRUCTIONS.contains(&&variant.ident.to_string().as_str()))
		.map(|variant| {
			let variant_name = &variant.ident;
			let method_name_string = &variant_name.to_string().to_snake_case();
			let method_name = syn::Ident::new(&method_name_string, variant_name.span());
			let docs = get_doc_comments(&variant);
			let method = match &variant.fields {
				Fields::Unnamed(fields) => {
					let arg_names: Vec<_> = fields
						.unnamed
						.iter()
						.enumerate()
						.map(|(index, _)| format_ident!("arg{}", index))
						.collect();
					let arg_types: Vec<_> = fields.unnamed.iter().map(|field| &field.ty).collect();
					quote! {
						#(#docs)*
						pub fn #method_name(mut self, #(#arg_names: #arg_types),*) -> #loaded_holding_builder_ident<Call> {
							#loaded_holding_builder_ident::<Call>::with_instructions(vec![
								#name::<Call>::#variant_name(#(#arg_names),*)
							])
						}
					}
				},
				Fields::Named(fields) => {
					let arg_names: Vec<_> = fields.named.iter().map(|field| &field.ident).collect();
					let arg_types: Vec<_> = fields.named.iter().map(|field| &field.ty).collect();
					quote! {
						#(#docs)*
						pub fn #method_name(self, #(#arg_names: #arg_types),*) -> #loaded_holding_builder_ident<Call> {
							#loaded_holding_builder_ident::<Call>::with_instructions(vec![
								#name::<Call>::#variant_name { #(#arg_names),* }
							])
						}
					}
				},
				_ => return Err(Error::new_spanned(&variant, "Instructions that load the holding register take operands"))
			};
			Ok(method)
		})
		.collect::<std::result::Result<Vec<_>, _>>()?;

	let first_impl = quote! {
		impl<Call> XcmBuilder<Call> {
			#(#load_holding_methods)*
		}
	};

	// Then we require fees to be paid
	let buy_execution_method = data_enum.variants.iter()
		.find(|variant| variant.ident.to_string() == "BuyExecution")
		.map_or(Err(Error::new_spanned(&data_enum.variants, "No BuyExecution instruction")), |variant| {
			let variant_name = &variant.ident;
			let method_name_string = &variant_name.to_string().to_snake_case();
			let method_name = syn::Ident::new(&method_name_string, variant_name.span());
			let docs = get_doc_comments(&variant);
			let fields = match &variant.fields {
				Fields::Named(fields) => {
					let arg_names: Vec<_> = fields.named.iter().map(|field| &field.ident).collect();
					let arg_types: Vec<_> = fields.named.iter().map(|field| &field.ty).collect();
					quote! {
						#(#docs)*
						pub fn #method_name(mut self, #(#arg_names: #arg_types),*) -> XcmBuilderRaw<Call> {
							self.0.extend_from_slice(&[
								#name::<Call>::#variant_name { #(#arg_names),* }
							]);
							XcmBuilderRaw::<Call>::with_instructions(self.0)
						}
					}
				},
				_ => return Err(Error::new_spanned(&variant, "BuyExecution takes named fields")),
			};
			Ok(fields)
		})?;

	let second_impl = quote! {
		pub struct #loaded_holding_builder_ident<Call>(pub Vec<#name<Call>>);
		impl<Call> #loaded_holding_builder_ident<Call> {
			#buy_execution_method

			pub(crate) fn with_instructions(instructions: Vec<#name<Call>>) -> Self {
				Self(instructions)
			}
		}
	};

	let output = quote! {
		#first_impl
		#second_impl
	};

	Ok(output)
}

fn generate_builder_unpaid_impl(name: &Ident, data_enum: &DataEnum) -> Result<TokenStream2> {
	let unpaid_execution_variant = data_enum.variants.iter()
		.find(|variant| variant.ident.to_string() == "UnpaidExecution")
		.unwrap();
	let unpaid_execution_ident = &unpaid_execution_variant.ident;
	let unpaid_execution_method_name = Ident::new(
		&unpaid_execution_ident.to_string().to_snake_case(),
		unpaid_execution_ident.span()
	);
	let docs = get_doc_comments(&unpaid_execution_variant);
	let fields = match &unpaid_execution_variant.fields {
		Fields::Named(fields) => fields,
		_ => return Err(Error::new_spanned(&unpaid_execution_variant, "UnpaidExecution should have named fields")),
	};
	let arg_names: Vec<_> = fields.named.iter().map(|field| &field.ident).collect();
	let arg_types: Vec<_> = fields.named.iter().map(|field| &field.ty).collect();
	Ok(quote! {
		impl<Call> XcmBuilderUnpaid<Call> {
			#(#docs)*
			pub fn #unpaid_execution_method_name(self, #(#arg_names: #arg_types),*) -> XcmBuilderRaw<Call> {
				XcmBuilderRaw::<Call>::with_instructions(vec![
					#name::<Call>::#unpaid_execution_ident { #(#arg_names),* }
				])
			}
		}
	})
}

fn get_doc_comments(variant: &Variant) -> Vec<TokenStream2> {
	variant
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
		.collect()
}
