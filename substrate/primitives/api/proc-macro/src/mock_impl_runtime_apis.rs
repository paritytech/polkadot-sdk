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
	extract_impl_trait, extract_parameter_names_types_and_borrows, generate_crate_access,
	prefix_function_with_trait, return_type_extract_type, AllowSelfRefInParameters,
	RequireQualifiedTraitPath,
};

use proc_macro2::{Span, TokenStream};

use quote::quote;

use syn::{
	fold::Fold,
	parse::{Error, Parse, ParseStream, Result},
	parse_macro_input,
	spanned::Spanned,
	Ident, ItemImpl, Type,
};

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
fn implement_common_api_traits(
	match_arms: Vec<TokenStream>,
	api_ids: Vec<TokenStream>,
	self_ty: Type,
) -> Result<TokenStream> {
	let crate_ = generate_crate_access();

	Ok(quote! {
		impl #self_ty {
			fn call_api(&self, function: &str, arguments: Vec<u8>) -> Vec<u8> {
				match function {
					#( #match_arms )*
					f => panic!("Unknown function: `{f}`"),
				}
			}
		}

		impl<Block: #crate_::BlockT> #crate_::CallApiAt<Block> for #self_ty {
			type StateBackend = #crate_::InMemoryBackend<#crate_::HashingFor<Block>>;

			fn call_api_at(
				&self,
				params: #crate_::CallApiAtParams<Block>,
			) -> std::result::Result<Vec<u8>, #crate_::ApiError> {
				let function = params.function;
				let arguments = params.arguments;

				Ok(self.call_api(function, arguments))
			}

			fn runtime_version_at(
				&self,
				_: Block::Hash,
			) -> std::result::Result<#crate_::RuntimeVersion, #crate_::ApiError> {
				Ok(#crate_::RuntimeVersion {
					apis: vec![ #( #api_ids, )* ].into(),
					..Default::default()
				})
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
				unimplemented!("`Core::initialize_extensions` not implemented for mocks")
			}
		}
	})
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

		let (param_names, param_types_and_borrows) = match extract_parameter_names_types_and_borrows(
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
		let ret_type = return_type_extract_type(&input.sig.output);

		let orig_block = input.block.clone();

		let match_arm_impl = quote! {
			let ( #( #param_names ),* ): ( #( #param_types ),* ) =
				#crate_::Decode::decode(&mut &arguments[..])
					.expect("Parameters not correctly encoded for mock");

			// Setup the types correctly with borrow.
			#( let #param_names  = #param_borrows #param_names; )*

			let __res__: #ret_type = (move || #orig_block)();

			#crate_::Encode::encode(&__res__)
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
	api_ids: Vec<TokenStream>,
}

/// Generate the runtime api implementations from the given trait implementations.
///
/// This folds the method names, changes the method parameters, method return type,
/// extracts the error type, self type and the block type.
fn generate_runtime_api_impls(impls: &[ItemImpl]) -> Result<GeneratedRuntimeApiImpls> {
	let mut match_arms = Vec::with_capacity(impls.len());
	let mut api_ids = Vec::new();
	let mut self_ty: Option<Box<Type>> = None;
	let crate_ = generate_crate_access();

	for impl_ in impls {
		let impl_trait_path = extract_impl_trait(impl_, RequireQualifiedTraitPath::No)?;
		let impl_trait_ident = &impl_trait_path
			.segments
			.last()
			.ok_or_else(|| Error::new(impl_trait_path.span(), "Empty trait path not possible!"))?
			.ident;

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

		api_ids.push(quote! {
			(
				<dyn #impl_trait_path as #crate_::RuntimeApiInfo>::ID,
				<dyn #impl_trait_path as #crate_::RuntimeApiInfo>::VERSION
			)
		});

		FoldRuntimeApiImpl { match_arms: &mut match_arms, trait_: impl_trait_ident }
			.process(impl_.clone());
	}

	Ok(GeneratedRuntimeApiImpls {
		match_arms,
		self_ty: *self_ty.expect("There is at least one runtime api; qed"),
		api_ids,
	})
}

/// The implementation of the `mock_impl_runtime_apis!` macro.
pub fn mock_impl_runtime_apis_impl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
	// Parse all impl blocks
	let RuntimeApiImpls { impls: api_impls } = parse_macro_input!(input as RuntimeApiImpls);

	let mock =
		mock_impl_runtime_apis_impl_inner(&api_impls).unwrap_or_else(|e| e.to_compile_error());

	let mock_expanded = expander::Expander::new("impl_runtime_apis")
		.dry(std::env::var("SP_API_EXPAND").is_err())
		.verbose(true)
		.write_to_out_dir(mock)
		.expect("Does not fail because of IO in OUT_DIR; qed");

	mock_expanded.into()
}

fn mock_impl_runtime_apis_impl_inner(api_impls: &[ItemImpl]) -> Result<TokenStream> {
	let GeneratedRuntimeApiImpls { match_arms, self_ty, api_ids } =
		generate_runtime_api_impls(api_impls)?;
	let api_traits = implement_common_api_traits(match_arms, api_ids, self_ty)?;

	Ok(quote!(
		#api_traits
	))
}
