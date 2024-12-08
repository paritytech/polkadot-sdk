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
	Data, DataEnum, DeriveInput, Error, Expr, ExprLit, Fields, Ident, Lit, Meta, MetaNameValue,
	Result, Variant,
};

pub fn derive(input: DeriveInput) -> Result<TokenStream2> {
	let data_enum = match &input.data {
		Data::Enum(data_enum) => data_enum,
		_ => return Err(Error::new_spanned(&input, "Expected the `Instruction` enum")),
	};
	let builder_raw_impl = generate_builder_raw_impl(&input.ident, data_enum);
	let builder_impl = generate_builder_impl(&input.ident, data_enum)?;
	let builder_unpaid_impl = generate_builder_unpaid_impl(&input.ident, data_enum)?;
	let output = quote! {
		/// A trait for types that track state inside the XcmBuilder
		pub trait XcmBuilderState {}

		/// Access to all the instructions
		pub enum AnythingGoes {}
		/// You need to pay for execution
		pub enum PaymentRequired {}
		/// The holding register was loaded, now to buy execution
		pub enum LoadedHolding {}
		/// Need to explicitly state it won't pay for fees
		pub enum ExplicitUnpaidRequired {}

		impl XcmBuilderState for AnythingGoes {}
		impl XcmBuilderState for PaymentRequired {}
		impl XcmBuilderState for LoadedHolding {}
		impl XcmBuilderState for ExplicitUnpaidRequired {}

		/// Type used to build XCM programs
		pub struct XcmBuilder<Call, S: XcmBuilderState> {
			pub(crate) instructions: Vec<Instruction<Call>>,
			pub state: core::marker::PhantomData<S>,
		}

		impl<Call> Xcm<Call> {
			pub fn builder() -> XcmBuilder<Call, PaymentRequired> {
				XcmBuilder::<Call, PaymentRequired> {
					instructions: Vec::new(),
					state: core::marker::PhantomData,
				}
			}
			pub fn builder_unpaid() -> XcmBuilder<Call, ExplicitUnpaidRequired> {
				XcmBuilder::<Call, ExplicitUnpaidRequired> {
					instructions: Vec::new(),
					state: core::marker::PhantomData,
				}
			}
			pub fn builder_unsafe() -> XcmBuilder<Call, AnythingGoes> {
				XcmBuilder::<Call, AnythingGoes> {
					instructions: Vec::new(),
					state: core::marker::PhantomData,
				}
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
		let method_name = syn::Ident::new(method_name_string, variant_name.span());
		let docs = get_doc_comments(variant);
		let method = match &variant.fields {
			Fields::Unit => {
				quote! {
					pub fn #method_name(mut self) -> Self {
						self.instructions.push(#name::<Call>::#variant_name);
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
					pub fn #method_name(mut self, #(#arg_names: impl Into<#arg_types>),*) -> Self {
						#(let #arg_names = #arg_names.into();)*
						self.instructions.push(#name::<Call>::#variant_name(#(#arg_names),*));
						self
					}
				}
			},
			Fields::Named(fields) => {
				let arg_names: Vec<_> = fields.named.iter().map(|field| &field.ident).collect();
				let arg_types: Vec<_> = fields.named.iter().map(|field| &field.ty).collect();
				quote! {
					pub fn #method_name(mut self, #(#arg_names: impl Into<#arg_types>),*) -> Self {
						#(let #arg_names = #arg_names.into();)*
						self.instructions.push(#name::<Call>::#variant_name { #(#arg_names),* });
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
		impl<Call> XcmBuilder<Call, AnythingGoes> {
			#(#methods)*

			pub fn build(self) -> Xcm<Call> {
				Xcm(self.instructions)
			}
		}
	};
	output
}

fn generate_builder_impl(name: &Ident, data_enum: &DataEnum) -> Result<TokenStream2> {
	// We first require an instruction that load the holding register
	let load_holding_variants = data_enum
		.variants
		.iter()
		.map(|variant| {
			let maybe_builder_attr = variant.attrs.iter().find(|attr| match attr.meta {
				Meta::List(ref list) => list.path.is_ident("builder"),
				_ => false,
			});
			let builder_attr = match maybe_builder_attr {
				Some(builder) => builder.clone(),
				None => return Ok(None), /* It's not going to be an instruction that loads the
				                          * holding register */
			};
			let Meta::List(ref list) = builder_attr.meta else { unreachable!("We checked before") };
			let inner_ident: Ident = syn::parse2(list.tokens.clone()).map_err(|_| {
				Error::new_spanned(
					&builder_attr,
					"Expected `builder(loads_holding)` or `builder(pays_fees)`",
				)
			})?;
			let ident_to_match: Ident = syn::parse_quote!(loads_holding);
			if inner_ident == ident_to_match {
				Ok(Some(variant))
			} else {
				Ok(None) // Must have been `pays_fees` instead.
			}
		})
		.collect::<Result<Vec<_>>>()?;

	let load_holding_methods = load_holding_variants
		.into_iter()
		.flatten()
		.map(|variant| {
			let variant_name = &variant.ident;
			let method_name_string = &variant_name.to_string().to_snake_case();
			let method_name = syn::Ident::new(method_name_string, variant_name.span());
			let docs = get_doc_comments(variant);
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
						pub fn #method_name(self, #(#arg_names: impl Into<#arg_types>),*) -> XcmBuilder<Call, LoadedHolding> {
							let mut new_instructions = self.instructions;
							#(let #arg_names = #arg_names.into();)*
							new_instructions.push(#name::<Call>::#variant_name(#(#arg_names),*));
							XcmBuilder {
								instructions: new_instructions,
								state: core::marker::PhantomData,
							}
						}
					}
				},
				Fields::Named(fields) => {
					let arg_names: Vec<_> = fields.named.iter().map(|field| &field.ident).collect();
					let arg_types: Vec<_> = fields.named.iter().map(|field| &field.ty).collect();
					quote! {
						#(#docs)*
						pub fn #method_name(self, #(#arg_names: impl Into<#arg_types>),*) -> XcmBuilder<Call, LoadedHolding> {
							let mut new_instructions = self.instructions;
							#(let #arg_names = #arg_names.into();)*
							new_instructions.push(#name::<Call>::#variant_name { #(#arg_names),* });
							XcmBuilder {
								instructions: new_instructions,
								state: core::marker::PhantomData,
							}
						}
					}
				},
				_ =>
					return Err(Error::new_spanned(
						variant,
						"Instructions that load the holding register should take operands",
					)),
			};
			Ok(method)
		})
		.collect::<std::result::Result<Vec<_>, _>>()?;

	let first_impl = quote! {
		impl<Call> XcmBuilder<Call, PaymentRequired> {
			#(#load_holding_methods)*
		}
	};

	// Some operations are allowed after the holding register is loaded
	let allowed_after_load_holding_methods: Vec<TokenStream2> = data_enum
		.variants
		.iter()
		.filter(|variant| variant.ident == "ClearOrigin")
		.map(|variant| {
			let variant_name = &variant.ident;
			let method_name_string = &variant_name.to_string().to_snake_case();
			let method_name = syn::Ident::new(method_name_string, variant_name.span());
			let docs = get_doc_comments(variant);
			let method = match &variant.fields {
				Fields::Unit => {
					quote! {
						#(#docs)*
						pub fn #method_name(mut self) -> XcmBuilder<Call, LoadedHolding> {
							self.instructions.push(#name::<Call>::#variant_name);
							self
						}
					}
				},
				_ => return Err(Error::new_spanned(variant, "ClearOrigin should have no fields")),
			};
			Ok(method)
		})
		.collect::<std::result::Result<Vec<_>, _>>()?;

	// Then we require fees to be paid
	let pay_fees_variants = data_enum
		.variants
		.iter()
		.map(|variant| {
			let maybe_builder_attr = variant.attrs.iter().find(|attr| match attr.meta {
				Meta::List(ref list) => list.path.is_ident("builder"),
				_ => false,
			});
			let builder_attr = match maybe_builder_attr {
				Some(builder) => builder.clone(),
				None => return Ok(None), /* It's not going to be an instruction that pays fees */
			};
			let Meta::List(ref list) = builder_attr.meta else { unreachable!("We checked before") };
			let inner_ident: Ident = syn::parse2(list.tokens.clone()).map_err(|_| {
				Error::new_spanned(
					&builder_attr,
					"Expected `builder(loads_holding)` or `builder(pays_fees)`",
				)
			})?;
			let ident_to_match: Ident = syn::parse_quote!(pays_fees);
			if inner_ident == ident_to_match {
				Ok(Some(variant))
			} else {
				Ok(None) // Must have been `loads_holding` instead.
			}
		})
		.collect::<Result<Vec<_>>>()?;

	let pay_fees_methods = pay_fees_variants
		.into_iter()
		.flatten()
		.map(|variant| {
			let variant_name = &variant.ident;
			let method_name_string = &variant_name.to_string().to_snake_case();
			let method_name = syn::Ident::new(method_name_string, variant_name.span());
			let docs = get_doc_comments(variant);
			let fields = match &variant.fields {
				Fields::Named(fields) => {
					let arg_names: Vec<_> =
						fields.named.iter().map(|field| &field.ident).collect();
					let arg_types: Vec<_> =
						fields.named.iter().map(|field| &field.ty).collect();
					quote! {
						#(#docs)*
						pub fn #method_name(self, #(#arg_names: impl Into<#arg_types>),*) -> XcmBuilder<Call, AnythingGoes> {
							let mut new_instructions = self.instructions;
							#(let #arg_names = #arg_names.into();)*
							new_instructions.push(#name::<Call>::#variant_name { #(#arg_names),* });
							XcmBuilder {
								instructions: new_instructions,
								state: core::marker::PhantomData,
							}
						}
					}
				},
				_ =>
					return Err(Error::new_spanned(
						variant,
						"Both BuyExecution and PayFees have named fields",
					)),
			};
			Ok(fields)
		})
		.collect::<Result<Vec<_>>>()?;

	let second_impl = quote! {
		impl<Call> XcmBuilder<Call, LoadedHolding> {
			#(#allowed_after_load_holding_methods)*
			#(#pay_fees_methods)*
		}
	};

	let output = quote! {
		#first_impl
		#second_impl
	};

	Ok(output)
}

fn generate_builder_unpaid_impl(name: &Ident, data_enum: &DataEnum) -> Result<TokenStream2> {
	let unpaid_execution_variant = data_enum
		.variants
		.iter()
		.find(|variant| variant.ident == "UnpaidExecution")
		.ok_or(Error::new_spanned(&data_enum.variants, "No UnpaidExecution instruction"))?;
	let unpaid_execution_ident = &unpaid_execution_variant.ident;
	let unpaid_execution_method_name = Ident::new(
		&unpaid_execution_ident.to_string().to_snake_case(),
		unpaid_execution_ident.span(),
	);
	let docs = get_doc_comments(unpaid_execution_variant);
	let fields = match &unpaid_execution_variant.fields {
		Fields::Named(fields) => fields,
		_ =>
			return Err(Error::new_spanned(
				unpaid_execution_variant,
				"UnpaidExecution should have named fields",
			)),
	};
	let arg_names: Vec<_> = fields.named.iter().map(|field| &field.ident).collect();
	let arg_types: Vec<_> = fields.named.iter().map(|field| &field.ty).collect();
	Ok(quote! {
		impl<Call> XcmBuilder<Call, ExplicitUnpaidRequired> {
			#(#docs)*
			pub fn #unpaid_execution_method_name(self, #(#arg_names: impl Into<#arg_types>),*) -> XcmBuilder<Call, AnythingGoes> {
				let mut new_instructions = self.instructions;
				#(let #arg_names = #arg_names.into();)*
				new_instructions.push(#name::<Call>::#unpaid_execution_ident { #(#arg_names),* });
				XcmBuilder {
					instructions: new_instructions,
					state: core::marker::PhantomData,
				}
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
