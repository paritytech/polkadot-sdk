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
// limitations under the License

use crate::construct_runtime::Pallet;
use proc_macro2::TokenStream;
use quote::quote;
use syn::Ident;

pub fn expand_outer_inherent(
	runtime: &Ident,
	block: &TokenStream,
	unchecked_extrinsic: &TokenStream,
	pallet_decls: &[Pallet],
	scrate: &TokenStream,
) -> TokenStream {
	let mut pallet_positions = Vec::new();
	let mut pallet_names = Vec::new();
	let mut pallet_attrs = Vec::new();
	let mut query_inherent_part_macros = Vec::new();

	for (pallet_pos, pallet_decl) in pallet_decls
		.iter()
		.filter(|pallet_decl| pallet_decl.exists_part("Inherent"))
		.enumerate()
	{
		let name = &pallet_decl.name;
		let path = &pallet_decl.path;
		let attr = pallet_decl.get_attributes();

		pallet_positions.push(pallet_pos);
		pallet_names.push(name);
		pallet_attrs.push(attr);
		query_inherent_part_macros.push(quote! {
			#path::__substrate_inherent_check::is_inherent_part_defined!(#name);
		});
	}
	let pallet_count = pallet_positions.len();

	quote! {
		#( #query_inherent_part_macros )*

		trait InherentDataExt {
			fn create_extrinsics(&self) ->
				#scrate::__private::Vec<<#block as #scrate::sp_runtime::traits::Block>::Extrinsic>;
			fn check_extrinsics(&self, block: &#block) -> #scrate::inherent::CheckInherentsResult;
		}

		impl InherentDataExt for #scrate::inherent::InherentData {
			fn create_extrinsics(&self) ->
				#scrate::__private::Vec<<#block as #scrate::sp_runtime::traits::Block>::Extrinsic>
			{
				use #scrate::{inherent::ProvideInherent, traits::InherentBuilder};

				let mut inherents = #scrate::__private::Vec::new();

				#(
					#pallet_attrs
					if let Some(inherent) = #pallet_names::create_inherent(self) {
						let inherent = <#unchecked_extrinsic as InherentBuilder>::new_inherent(
							inherent.into(),
						);

						inherents.push(inherent);
					}
				)*

				inherents
			}

			fn check_extrinsics(&self, block: &#block) -> #scrate::inherent::CheckInherentsResult {
				use #scrate::inherent::{ProvideInherent, IsFatalError};
				use #scrate::traits::IsSubType;
				use #scrate::sp_runtime::traits::{Block as _, ExtrinsicCall};
				use #scrate::__private::{sp_inherents::Error, log};

				let mut result = #scrate::inherent::CheckInherentsResult::new();

				// This handle assume we abort on the first fatal error.
				fn handle_put_error_result(res: Result<(), Error>) {
					const LOG_TARGET: &str = "runtime::inherent";
					match res {
						Ok(()) => (),
						Err(Error::InherentDataExists(id)) =>
							log::debug!(
								target: LOG_TARGET,
								"Some error already reported for inherent {:?}, new non fatal \
								error is ignored",
								id
							),
						Err(Error::FatalErrorReported) =>
							log::error!(
								target: LOG_TARGET,
								"Fatal error already reported, unexpected considering there is \
								only one fatal error",
							),
						Err(_) =>
							log::error!(
								target: LOG_TARGET,
								"Unexpected error from `put_error` operation",
							),
					}
				}

				let mut pallet_has_inherent = [false; #pallet_count];
				for xt in block.extrinsics() {
					// Inherents are before any other extrinsics.
					// And signed extrinsics are not inherents.
					if !(#scrate::sp_runtime::traits::ExtrinsicLike::is_bare(xt)) {
						break
					}

					let mut is_inherent = false;
					let call = ExtrinsicCall::call(xt);
					#(
						#pallet_attrs
						{
							if let Some(call) = IsSubType::<_>::is_sub_type(call) {
								if #pallet_names::is_inherent(call) {
									is_inherent = true;
									pallet_has_inherent[#pallet_positions] = true;
									if let Err(e) = #pallet_names::check_inherent(call, self) {
										handle_put_error_result(result.put_error(
											#pallet_names::INHERENT_IDENTIFIER, &e
										));
										if e.is_fatal_error() {
											return result;
										}
									}
								}
							}
						}
					)*

					// Inherents are before any other extrinsics.
					// No module marked it as inherent, thus it is not.
					if !is_inherent {
						break
					}
				}

				#(
					#pallet_attrs
					match #pallet_names::is_inherent_required(self) {
						Ok(Some(e)) => {
							if !pallet_has_inherent[#pallet_positions] {
								handle_put_error_result(result.put_error(
									#pallet_names::INHERENT_IDENTIFIER, &e
								));
								if e.is_fatal_error() {
									return result;
								}
							}
						},
						Ok(None) => (),
						Err(e) => {
							handle_put_error_result(result.put_error(
								#pallet_names::INHERENT_IDENTIFIER, &e
							));
							if e.is_fatal_error() {
								return result;
							}
						},
					}
				)*

				result
			}
		}

		impl #scrate::traits::IsInherent<<#block as #scrate::sp_runtime::traits::Block>::Extrinsic> for #runtime {
			fn is_inherent(ext: &<#block as #scrate::sp_runtime::traits::Block>::Extrinsic) -> bool {
				use #scrate::inherent::ProvideInherent;
				use #scrate::traits::IsSubType;
				use #scrate::sp_runtime::traits::ExtrinsicCall;

				let is_bare = #scrate::sp_runtime::traits::ExtrinsicLike::is_bare(ext);
				if !is_bare {
					// Inherents must be bare extrinsics.
					return false
				}

				let call = ExtrinsicCall::call(ext);
				#(
					#pallet_attrs
					{
						if let Some(call) = IsSubType::<_>::is_sub_type(call) {
							if <#pallet_names as ProvideInherent>::is_inherent(&call) {
								return true;
							}
						}
					}
				)*
				false
			}
		}
	}
}
