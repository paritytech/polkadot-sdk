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
use std::str::FromStr;
use syn::Ident;

pub fn expand_outer_dispatch(
	runtime: &Ident,
	system_pallet: &Pallet,
	pallet_decls: &[Pallet],
	scrate: &TokenStream,
) -> TokenStream {
	let mut variant_defs = TokenStream::new();
	let mut variant_patterns = Vec::new();
	let mut query_call_part_macros = Vec::new();
	let mut pallet_names = Vec::new();
	let mut pallet_attrs = Vec::new();

	let mut checkpointed_call_data_variants = Vec::new();
	let mut checkpointed_call_data_conversions = TokenStream::new();

	let system_path = &system_pallet.path;

	let pallets_with_call = pallet_decls.iter().filter(|decl| decl.exists_part("Call"));

	for pallet_declaration in pallets_with_call {
		let instance = pallet_declaration.instance.as_ref();
		let name = &pallet_declaration.name;
		let path = &pallet_declaration.path;
		let index = pallet_declaration.index;
		let attr =
			pallet_declaration.cfg_pattern.iter().fold(TokenStream::new(), |acc, pattern| {
				let attr = TokenStream::from_str(&format!("#[cfg({})]", pattern.original()))
					.expect("was successfully parsed before; qed");
				quote! {
					#acc
					#attr
				}
			});

		variant_defs.extend(quote! {
			#attr
			#[codec(index = #index)]
			#name( #scrate::dispatch::CallableCallFor<#name, #runtime> ),
		});
		variant_patterns.push(quote!(RuntimeCall::#name(call)));
		pallet_names.push(name);
		pallet_attrs.push(attr);
		query_call_part_macros.push(quote! {
			#path::__substrate_call_check::is_call_part_defined!(#name);
		});

		checkpointed_call_data_variants.push(expand_checkpointed_call_data_variant(
			runtime,
			pallet_declaration,
			index,
			instance,
		));
		checkpointed_call_data_conversions.extend(expand_checkpointed_call_data_conversions(
			scrate,
			runtime,
			pallet_declaration,
			instance,
		));
	}

	quote! {
		#( #query_call_part_macros )*

		#[derive(
			Clone, PartialEq, Eq,
			#scrate::__private::codec::Encode,
			#scrate::__private::codec::Decode,
			#scrate::__private::scale_info::TypeInfo,
			#scrate::__private::RuntimeDebug,
		)]
		pub enum RuntimeCall {
			#variant_defs
		}
		#[cfg(test)]
		impl RuntimeCall {
			/// Return a list of the module names together with their size in memory.
			pub const fn sizes() -> &'static [( &'static str, usize )] {
				use #scrate::dispatch::Callable;
				use core::mem::size_of;
				&[#(
					#pallet_attrs
					(
						stringify!(#pallet_names),
						size_of::< <#pallet_names as Callable<#runtime>>::RuntimeCall >(),
					),
				)*]
			}

			/// Panics with diagnostic information if the size is greater than the given `limit`.
			pub fn assert_size_under(limit: usize) {
				let size = core::mem::size_of::<Self>();
				let call_oversize = size > limit;
				if call_oversize {
					println!("Size of `Call` is {} bytes (provided limit is {} bytes)", size, limit);
					let mut sizes = Self::sizes().to_vec();
					sizes.sort_by_key(|x| -(x.1 as isize));
					for (i, &(name, size)) in sizes.iter().enumerate().take(5) {
						println!("Offender #{}: {} at {} bytes", i + 1, name, size);
					}
					if let Some((_, next_size)) = sizes.get(5) {
						println!("{} others of size {} bytes or less", sizes.len() - 5, next_size);
					}
					panic!(
						"Size of `Call` is more than limit; use `Box` on complex parameter types to reduce the
						size of `Call`.
						If the limit is too strong, maybe consider providing a higher limit."
					);
				}
			}
		}
		impl #scrate::dispatch::GetDispatchInfo for RuntimeCall {
			fn get_dispatch_info(&self) -> #scrate::dispatch::DispatchInfo {
				match self {
					#(
						#pallet_attrs
						#variant_patterns => call.get_dispatch_info(),
					)*
				}
			}
		}

		impl #scrate::dispatch::CheckIfFeeless for RuntimeCall {
			type Origin = #system_path::pallet_prelude::OriginFor<#runtime>;
			type CheckpointedCallData = RuntimeCheckpointedCallData;
			fn is_feeless(&self, origin: &Self::Origin) -> (bool, Option<Self::CheckpointedCallData>) {
				match self {
					#(
						#pallet_attrs
						#variant_patterns => {
							let result = call.is_feeless(origin);
							(result.0, result.1.map_or(None, |data| Some(data.into())))
						}
					)*
				}
			}
		}

		impl #scrate::traits::GetCallMetadata for RuntimeCall {
			fn get_call_metadata(&self) -> #scrate::traits::CallMetadata {
				use #scrate::traits::GetCallName;
				match self {
					#(
						#pallet_attrs
						#variant_patterns => {
							let function_name = call.get_call_name();
							let pallet_name = stringify!(#pallet_names);
							#scrate::traits::CallMetadata { function_name, pallet_name }
						}
					)*
				}
			}

			fn get_module_names() -> &'static [&'static str] {
				&[#(
					#pallet_attrs
					stringify!(#pallet_names),
				)*]
			}

			fn get_call_names(module: &str) -> &'static [&'static str] {
				use #scrate::{dispatch::Callable, traits::GetCallName};
				match module {
					#(
						#pallet_attrs
						stringify!(#pallet_names) =>
							<<#pallet_names as Callable<#runtime>>::RuntimeCall
								as GetCallName>::get_call_names(),
					)*
					_ => unreachable!(),
				}
			}
		}
		impl #scrate::__private::Dispatchable for RuntimeCall {
			type RuntimeOrigin = RuntimeOrigin;
			type Config = RuntimeCall;
			type Info = #scrate::dispatch::DispatchInfo;
			type PostInfo = #scrate::dispatch::PostDispatchInfo;
			fn dispatch(self, origin: RuntimeOrigin) -> #scrate::dispatch::DispatchResultWithPostInfo {
				if !<Self::RuntimeOrigin as #scrate::traits::OriginTrait>::filter_call(&origin, &self) {
					return ::core::result::Result::Err(
						#system_path::Error::<#runtime>::CallFiltered.into()
					);
				}

				#scrate::traits::UnfilteredDispatchable::dispatch_bypass_filter(self, origin)
			}
		}
		impl #scrate::traits::UnfilteredDispatchable for RuntimeCall {
			type RuntimeOrigin = RuntimeOrigin;
			fn dispatch_bypass_filter(self, origin: RuntimeOrigin) -> #scrate::dispatch::DispatchResultWithPostInfo {
				match self {
					#(
						#pallet_attrs
						#variant_patterns =>
							#scrate::traits::UnfilteredDispatchable::dispatch_bypass_filter(call, origin),
					)*
				}
			}
		}

		#(
			#pallet_attrs
			impl #scrate::traits::IsSubType<#scrate::dispatch::CallableCallFor<#pallet_names, #runtime>> for RuntimeCall {
				#[allow(unreachable_patterns)]
				fn is_sub_type(&self) -> Option<&#scrate::dispatch::CallableCallFor<#pallet_names, #runtime>> {
					match self {
						#variant_patterns => Some(call),
						// May be unreachable
						_ => None,
					}
				}
			}

			#pallet_attrs
			impl From<#scrate::dispatch::CallableCallFor<#pallet_names, #runtime>> for RuntimeCall {
				fn from(call: #scrate::dispatch::CallableCallFor<#pallet_names, #runtime>) -> Self {
					#variant_patterns
				}
			}
		)*


		#[derive(
			Clone, PartialEq, Eq,
			#scrate::__private::codec::Encode,
			#scrate::__private::codec::Decode,
			#scrate::__private::scale_info::TypeInfo,
			#scrate::__private::RuntimeDebug,
		)]
		pub enum RuntimeCheckpointedCallData {
			#(
				#checkpointed_call_data_variants,
			)*
			Void
		}

		#checkpointed_call_data_conversions
	}
}

fn expand_checkpointed_call_data_variant(
	runtime: &Ident,
	pallet: &Pallet,
	index: u8,
	instance: Option<&Ident>,
) -> TokenStream {
	let variant_name = &pallet.name;
	let path = &pallet.path;

	let pallet_checkpointed_call_data = match instance {
		Some(inst) => quote! { #path::CheckpointedCallData<#runtime, #path::#inst> },
		None => quote! { #path::CheckpointedCallData<#runtime> },
	};

	let attr = pallet.cfg_pattern.iter().fold(TokenStream::new(), |acc, pattern| {
		let attr = TokenStream::from_str(&format!("#[cfg({})]", pattern.original()))
			.expect("was successfully parsed before; qed");
		quote! {
			#acc
			#attr
		}
	});

	quote! {
		#attr
		#[codec(index = #index)]
		#variant_name(#pallet_checkpointed_call_data)
	}
}

fn expand_checkpointed_call_data_conversions(
	scrate: &TokenStream,
	runtime: &Ident,
	pallet: &Pallet,
	instance: Option<&Ident>,
) -> TokenStream {
	let path = &pallet.path;
	let variant_name = &pallet.name;

	let pallet_checkpointed_call_data = match instance {
		Some(inst) => quote! { #path::CheckpointedCallData<#runtime, #path::#inst> },
		None => quote! { #path::CheckpointedCallData<#runtime> },
	};

	let attr = pallet.cfg_pattern.iter().fold(TokenStream::new(), |acc, pattern| {
		let attr = TokenStream::from_str(&format!("#[cfg({})]", pattern.original()))
			.expect("was successfully parsed before; qed");
		quote! {
			#acc
			#attr
		}
	});

	quote! {
		#attr
		impl From<#pallet_checkpointed_call_data> for RuntimeCheckpointedCallData {
			fn from(x: #pallet_checkpointed_call_data) -> Self {
				RuntimeCheckpointedCallData::#variant_name(x)
			}
		}

		#attr
		impl TryFrom<RuntimeCheckpointedCallData> for #pallet_checkpointed_call_data {
			type Error = ();
			fn try_from(
				x: RuntimeCheckpointedCallData,
			) -> #scrate::__private::sp_std::result::Result<#pallet_checkpointed_call_data, ()> {
				if let RuntimeCheckpointedCallData::#variant_name(l) = x {
					Ok(l)
				} else {
					Err(())
				}
			}
		}
	}
}
