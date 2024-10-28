//! Contains logic for expanding task-related items.

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

//! Home of the expansion code for the Tasks API

use crate::pallet::{parse::tasks::*, Def};
use inflector::Inflector;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote, ToTokens};
use syn::{parse_quote_spanned, spanned::Spanned};

impl TaskEnumDef {
	/// Since we optionally allow users to manually specify a `#[pallet::task_enum]`, in the
	/// event they _don't_ specify one (which is actually the most common behavior) we have to
	/// generate one based on the existing [`TasksDef`]. This method performs that generation.
	pub fn generate(tasks: &TasksDef, def: &Def) -> Self {
		// We use the span of the attribute to indicate that the error comes from code generated
		// for the specific section, otherwise the item impl.
		let span = tasks
			.tasks_attr
			.as_ref()
			.map_or_else(|| tasks.item_impl.span(), |attr| attr.span());

		let type_decl_bounded_generics = def.type_decl_bounded_generics(span);

		let variants = if tasks.tasks_attr.is_some() {
			tasks
				.tasks
				.iter()
				.map(|task| {
					let ident = &task.item.sig.ident;
					let ident =
						format_ident!("{}", ident.to_string().to_class_case(), span = ident.span());

					let args = task.item.sig.inputs.iter().collect::<Vec<_>>();

					if args.is_empty() {
						quote!(#ident)
					} else {
						quote!(#ident {
							#(#args),*
						})
					}
				})
				.collect::<Vec<_>>()
		} else {
			Vec::new()
		};

		parse_quote_spanned! { span =>
			/// Auto-generated enum that encapsulates all tasks defined by this pallet.
			///
			/// Conceptually similar to the [`Call`] enum, but for tasks. This is only
			/// generated if there are tasks present in this pallet.
			#[pallet::task_enum]
			pub enum Task<#type_decl_bounded_generics> {
				#(
					#variants,
				)*
			}
		}
	}
}

impl TaskEnumDef {
	fn expand_to_tokens(&self, def: &Def) -> TokenStream2 {
		if let Some(attr) = &self.attr {
			let ident = &self.item_enum.ident;
			let vis = &self.item_enum.vis;
			let attrs = &self.item_enum.attrs;
			let generics = &self.item_enum.generics;
			let variants = &self.item_enum.variants;
			let frame_support = &def.frame_support;
			let type_use_generics = &def.type_use_generics(attr.span());
			let type_impl_generics = &def.type_impl_generics(attr.span());

			// `item_enum` is short-hand / generated enum
			quote! {
				#(#attrs)*
				#[derive(
					#frame_support::CloneNoBound,
					#frame_support::EqNoBound,
					#frame_support::PartialEqNoBound,
					#frame_support::pallet_prelude::Encode,
					#frame_support::pallet_prelude::Decode,
					#frame_support::pallet_prelude::TypeInfo,
				)]
				#[codec(encode_bound())]
				#[codec(decode_bound())]
				#[scale_info(skip_type_params(#type_use_generics))]
				#vis enum #ident #generics {
					#variants
					#[doc(hidden)]
					#[codec(skip)]
					__Ignore(core::marker::PhantomData<(#type_use_generics)>, #frame_support::Never),
				}

				impl<#type_impl_generics> core::fmt::Debug for #ident<#type_use_generics> {
					fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
						f.debug_struct(stringify!(#ident)).field("value", self).finish()
					}
				}
			}
		} else {
			// `item_enum` is a manually specified enum (no attribute)
			self.item_enum.to_token_stream()
		}
	}
}

impl TasksDef {
	fn expand_to_tokens(&self, def: &Def) -> TokenStream2 {
		let frame_support = &def.frame_support;
		let enum_ident = syn::Ident::new("Task", self.enum_ident.span());
		let enum_arguments = &self.enum_arguments;
		let enum_use = quote!(#enum_ident #enum_arguments);

		let task_fn_idents = self
			.tasks
			.iter()
			.map(|task| {
				format_ident!(
					"{}",
					&task.item.sig.ident.to_string().to_class_case(),
					span = task.item.sig.ident.span()
				)
			})
			.collect::<Vec<_>>();
		let task_indices = self.tasks.iter().map(|task| &task.index_attr.meta.index);
		let task_conditions = self.tasks.iter().map(|task| &task.condition_attr.meta.expr);
		let task_weights = self.tasks.iter().map(|task| &task.weight_attr.meta.expr);
		let task_iters = self.tasks.iter().map(|task| &task.list_attr.meta.expr);

		let task_fn_impls = self.tasks.iter().map(|task| {
			let mut task_fn_impl = task.item.clone();
			task_fn_impl.attrs = vec![];
			task_fn_impl
		});

		let task_fn_names = self.tasks.iter().map(|task| &task.item.sig.ident);
		let task_arg_names = self.tasks.iter().map(|task| &task.arg_names).collect::<Vec<_>>();

		let impl_generics = &self.item_impl.generics;
		quote! {
			impl #impl_generics #enum_use
			{
				#(#task_fn_impls)*
			}

			impl #impl_generics #frame_support::traits::Task for #enum_use
			{
				type Enumeration = #frame_support::__private::IntoIter<#enum_use>;

				fn iter() -> Self::Enumeration {
					let mut all_tasks = #frame_support::__private::vec![];
					#(all_tasks
						.extend(#task_iters.map(|(#(#task_arg_names),*)| #enum_ident::#task_fn_idents { #(#task_arg_names: #task_arg_names.clone()),* })
						.collect::<#frame_support::__private::Vec<_>>());
					)*
					all_tasks.into_iter()
				}

				fn task_index(&self) -> u32 {
					match self.clone() {
						#(#enum_ident::#task_fn_idents { .. } => #task_indices,)*
						Task::__Ignore(_, _) => unreachable!(),
					}
				}

				fn is_valid(&self) -> bool {
					match self.clone() {
						#(#enum_ident::#task_fn_idents { #(#task_arg_names),* } => (#task_conditions)(#(#task_arg_names),* ),)*
						Task::__Ignore(_, _) => unreachable!(),
					}
				}

				fn run(&self) -> Result<(), #frame_support::pallet_prelude::DispatchError> {
					match self.clone() {
						#(#enum_ident::#task_fn_idents { #(#task_arg_names),* } => {
							<#enum_use>::#task_fn_names(#( #task_arg_names, )* )
						},)*
						Task::__Ignore(_, _) => unreachable!(),
					}
				}

				#[allow(unused_variables)]
				fn weight(&self) -> #frame_support::pallet_prelude::Weight {
					match self.clone() {
						#(#enum_ident::#task_fn_idents { #(#task_arg_names),* } => #task_weights,)*
						Task::__Ignore(_, _) => unreachable!(),
					}
				}
			}
		}
	}
}

/// Generate code related to tasks.
pub fn expand_tasks(def: &Def) -> TokenStream2 {
	let Some(tasks_def) = &def.tasks else {
		return quote!();
	};

	let default_task_enum = TaskEnumDef::generate(&tasks_def, def);

	let task_enum = def.task_enum.as_ref().unwrap_or_else(|| &default_task_enum);

	let tasks_expansion = tasks_def.expand_to_tokens(def);
	let task_enum_expansion = task_enum.expand_to_tokens(def);

	quote! {
		#tasks_expansion
		#task_enum_expansion
	}
}
