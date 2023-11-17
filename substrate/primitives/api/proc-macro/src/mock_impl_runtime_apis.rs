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

use crate::utils::{
	extract_block_type_from_trait_path, extract_impl_trait,
	extract_parameter_names_types_and_borrows, generate_crate_access, prefix_function_with_trait,
	return_type_extract_type, AllowSelfRefInParameters, RequireQualifiedTraitPath,
};

use proc_macro2::{Span, TokenStream};

use quote::{quote, quote_spanned};

use syn::{
	fold::{self, Fold},
	parse::{Error, Parse, ParseStream, Result},
	parse_macro_input, parse_quote,
	spanned::Spanned,
	Attribute, Ident, ItemImpl, Pat, Type, TypePath,
};

/// The `advanced` attribute.
///
/// If this attribute is given to a function, the function gets access to the `Hash` as first
/// parameter and needs to return a `Result` with the appropriate error type.
const ADVANCED_ATTRIBUTE: &str = "advanced";

/// The structure used for parsing the runtime api implementations.
struct RuntimeApiImpls {
	impls: Vec<ItemImpl>,
}

impl Parse for RuntimeApiImpls {
	fn parse(input: ParseStream) -> Result<Self> {
		let mut impls = Vec::new();

		while !input.is_empty() {
			impls.push(ItemImpl::parse(input)?);
		}

		if impls.is_empty() {
			Err(Error::new(Span::call_site(), "No api implementation given!"))
		} else {
			Ok(Self { impls })
		}
	}
}

/// Implement the `ApiExt` trait and the `Core` runtime api.
fn implement_common_api_traits(match_arms: Vec<TokenStream>, self_ty: Type) -> Result<TokenStream> {
	let crate_ = generate_crate_access();

	Ok(quote!(
		impl<Block: #crate_::BlockT> #crate_::CallApiAt<Block> for #self_ty {
			type StateBackend = #crate_::InMemoryBackend<#crate_::HashingFor<Block>>;

			fn call_api_at(
				&self,
				params: #crate_::CallApiAtParams<Block>,
			) -> std::result::Result<Vec<u8>, #crate_::ApiError> {
				let function = params.function;
				let arguments = params.arguments;


			}

			fn runtime_version_at(
				&self,
				_: Block::Hash,
			) -> std::result::Result<#crate_::RuntimeVersion, #crate_::ApiError> {
				unimplemented!("`runtime_version_at` not implemented for mocks")
			}

			fn state_at(
				&self,
				_: Block::Hash,
			) -> std::result::Result<Self::StateBackend, #crate_::ApiError> {
				unimplemented!("`state_at` not implemented for mocks")
			}

			fn initialize_extensions(
				&self,
				_: Block::Hash,
				_: &mut #crate_::Extensions,
			) -> std::result::Result<(), #crate_::ApiError> {
				unimplemented!("`initialize_extensions` not implemented for mocks")
			}
		}
	))
}

/// Returns if the advanced attribute is present in the given `attributes`.
///
/// If the attribute was found, it will be automatically removed from the vec.
fn has_advanced_attribute(attributes: &mut Vec<Attribute>) -> bool {
	let mut found = false;
	attributes.retain(|attr| {
		if attr.path().is_ident(ADVANCED_ATTRIBUTE) {
			found = true;
			false
		} else {
			true
		}
	});

	found
}

/// Get the name and type of the `at` parameter that is passed to a runtime api function.
///
/// If `is_advanced` is `false`, the name is `_`.
fn get_at_param_name(
	is_advanced: bool,
	param_names: &mut Vec<Pat>,
	param_types_and_borrows: &mut Vec<(TokenStream, bool)>,
	function_span: Span,
	default_hash_type: &TokenStream,
) -> Result<(TokenStream, TokenStream)> {
	if is_advanced {
		if param_names.is_empty() {
			return Err(Error::new(
				function_span,
				format!(
					"If using the `{}` attribute, it is required that the function \
					 takes at least one argument, the `Hash`.",
					ADVANCED_ATTRIBUTE,
				),
			))
		}

		// `param_names` and `param_types` have the same length, so if `param_names` is not empty
		// `param_types` can not be empty as well.
		let ptype_and_borrows = param_types_and_borrows.remove(0);
		let span = ptype_and_borrows.1.span();
		if ptype_and_borrows.1 {
			return Err(Error::new(span, "`Hash` needs to be taken by value and not by reference!"))
		}

		let name = param_names.remove(0);
		Ok((quote!( #name ), ptype_and_borrows.0))
	} else {
		Ok((quote!(_), default_hash_type.clone()))
	}
}

/// Auxiliary structure to fold a runtime api trait implementation into the expected format.
///
/// This renames the methods, changes the method parameters and extracts the error type.
struct FoldRuntimeApiImpl<'a> {
	trait_: &'a Ident,
	match_arms: &'a mut Vec<TokenStream>,
}

impl<'a> FoldRuntimeApiImpl<'a> {
	/// Process the given [`syn::ItemImpl`].
	fn process(mut self, impl_item: syn::ItemImpl) -> syn::ItemImpl {
		self.fold_item_impl(impl_item)
	}
}

impl<'a> Fold for FoldRuntimeApiImpl<'a> {
	fn fold_impl_item_fn(&mut self, input: syn::ImplItemFn) -> syn::ImplItemFn {
		let crate_ = generate_crate_access();
		let mut errors = Vec::new();

		let (param_names, param_types_and_borrows) =
			match extract_parameter_names_types_and_borrows(
				&input.sig,
				AllowSelfRefInParameters::YesButIgnore,
			) {
				Ok(res) => (
					res.iter().map(|v| v.0.clone()).collect::<Vec<_>>(),
					res.iter().map(|v| (v.1.clone(), v.2.clone())).collect::<Vec<_>>(),
				),
				Err(e) => {
					errors.push(e.to_compile_error());

					(Default::default(), Default::default())
				},
			};

		let match_str = prefix_function_with_trait(self.trait_, &input.sig.ident);

		let param_types = param_types_and_borrows.iter().map(|v| &v.0);
		let param_borrows = param_types_and_borrows.iter().map(|v| &v.1);

		let orig_block = input.block.clone();

		let match_arm_impl = quote! {
			let ( #( #param_names ),* ): ( #( #param_types ),* ) =
				#crate_::Decode::decode(&mut &arguments[..])
					.expect("Parameters not correctly encoded for mock");

			// Setup the types correctly with borrow.
			#( let #param_names  = #param_borrows #param_names );*

			let __fn_implementation__ = move || #orig_block;

			#crate_::Encode::encode(&__fn_implementation__())
		};

		self.match_arms.push(quote! {
			#match_str => { #match_arm_impl },
		});

		input
	}
}

/// Result of [`generate_runtime_api_impls`].
struct GeneratedRuntimeApiImpls {
	/// All the runtime api implementations.
	match_arms: Vec<TokenStream>,
	/// The type the traits are implemented for.
	self_ty: Type,
}

/// Generate the runtime api implementations from the given trait implementations.
///
/// This folds the method names, changes the method parameters, method return type,
/// extracts the error type, self type and the block type.
fn generate_runtime_api_impls(impls: &[ItemImpl]) -> Result<GeneratedRuntimeApiImpls> {
	let mut match_arms = Vec::with_capacity(impls.len());
	let mut self_ty: Option<Box<Type>> = None;

	for impl_ in impls {
		let impl_trait_path = extract_impl_trait(impl_, RequireQualifiedTraitPath::No)?;
		let impl_trait_ident = &impl_trait_path
			.segments
			.last()
			.ok_or_else(|| Error::new(impl_trait_path.span(), "Empty trait path not possible!"))?
			.ident;
		let block_type = extract_block_type_from_trait_path(impl_trait_path)?;

		self_ty = match self_ty.take() {
			Some(self_ty) =>
				if self_ty == impl_.self_ty {
					Some(self_ty)
				} else {
					let mut error = Error::new(
						impl_.self_ty.span(),
						"Self type should not change between runtime apis",
					);

					error.combine(Error::new(self_ty.span(), "First self type found here"));

					return Err(error)
				},
			None => Some(impl_.self_ty.clone()),
		};

		FoldRuntimeApiImpl { match_arms: &mut match_arms, trait_: impl_trait_ident }
			.process(impl_.clone());
	}

	Ok(GeneratedRuntimeApiImpls {
		match_arms,
		self_ty: *self_ty.expect("There is at least one runtime api; qed"),
	})
}

/// The implementation of the `mock_impl_runtime_apis!` macro.
pub fn mock_impl_runtime_apis_impl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
	// Parse all impl blocks
	let RuntimeApiImpls { impls: api_impls } = parse_macro_input!(input as RuntimeApiImpls);

	mock_impl_runtime_apis_impl_inner(&api_impls)
		.unwrap_or_else(|e| e.to_compile_error())
		.into()
}

fn mock_impl_runtime_apis_impl_inner(api_impls: &[ItemImpl]) -> Result<TokenStream> {
	let GeneratedRuntimeApiImpls { match_arms, self_ty } =
		generate_runtime_api_impls(api_impls)?;
	let api_traits = implement_common_api_traits(match_arms, self_ty)?;

	Ok(quote!(
		#api_traits
	))
}
