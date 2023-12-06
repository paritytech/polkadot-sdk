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
use derive_syn_parse::Parse;
use inflector::Inflector;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote, ToTokens};
use syn::{parse_quote, spanned::Spanned, ItemEnum, ItemImpl};

impl TaskEnumDef {
	/// Since we optionally allow users to manually specify a `#[pallet::task_enum]`, in the
	/// event they _don't_ specify one (which is actually the most common behavior) we have to
	/// generate one based on the existing [`TasksDef`]. This method performs that generation.
	pub fn generate(
		tasks: &TasksDef,
		type_decl_bounded_generics: TokenStream2,
		type_use_generics: TokenStream2,
	) -> Self {
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
		let mut task_enum_def: TaskEnumDef = parse_quote! {
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
		};
		task_enum_def.type_use_generics = type_use_generics;
		task_enum_def
	}
}

impl ToTokens for TaskEnumDef {
	fn to_tokens(&self, tokens: &mut TokenStream2) {
		let item_enum = &self.item_enum;
		let ident = &item_enum.ident;
		let vis = &item_enum.vis;
		let attrs = &item_enum.attrs;
		let generics = &item_enum.generics;
		let variants = &item_enum.variants;
		let scrate = &self.scrate;
		let type_use_generics = &self.type_use_generics;
		if self.attr.is_some() {
			// `item_enum` is short-hand / generated enum
			tokens.extend(quote! {
				#(#attrs)*
				#[derive(
					#scrate::CloneNoBound,
					#scrate::EqNoBound,
					#scrate::PartialEqNoBound,
					#scrate::pallet_prelude::Encode,
					#scrate::pallet_prelude::Decode,
					#scrate::pallet_prelude::TypeInfo,
				)]
				#[codec(encode_bound())]
				#[codec(decode_bound())]
				#[scale_info(skip_type_params(#type_use_generics))]
				#vis enum #ident #generics {
					#variants
					#[doc(hidden)]
					#[codec(skip)]
					__Ignore(core::marker::PhantomData<T>, #scrate::Never),
				}

				impl<T: Config> core::fmt::Debug for #ident<#type_use_generics> {
					fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
						f.debug_struct(stringify!(#ident)).field("value", self).finish()
					}
				}
			});
		} else {
			// `item_enum` is a manually specified enum (no attribute)
			tokens.extend(item_enum.to_token_stream());
		}
	}
}

/// Represents an already-expanded [`TasksDef`].
#[derive(Parse)]
pub struct ExpandedTasksDef {
	pub task_item_impl: ItemImpl,
	pub task_trait_impl: ItemImpl,
}

impl ToTokens for TasksDef {
	fn to_tokens(&self, tokens: &mut TokenStream2) {
		let scrate = &self.scrate;
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

		let sp_std = quote!(#scrate::__private::sp_std);
		let impl_generics = &self.item_impl.generics;
		tokens.extend(quote! {
			impl #impl_generics #enum_use
			{
				#(#task_fn_impls)*
			}

			impl #impl_generics #scrate::traits::Task for #enum_use
			{
				type Enumeration = #sp_std::vec::IntoIter<#enum_use>;

				fn iter() -> Self::Enumeration {
					let mut all_tasks = #sp_std::vec![];
					#(all_tasks
						.extend(#task_iters.map(|(#(#task_arg_names),*)| #enum_ident::#task_fn_idents { #(#task_arg_names: #task_arg_names.clone()),* })
						.collect::<#sp_std::vec::Vec<_>>());
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

				fn run(&self) -> Result<(), #scrate::pallet_prelude::DispatchError> {
					match self.clone() {
						#(#enum_ident::#task_fn_idents { #(#task_arg_names),* } => {
							<#enum_use>::#task_fn_names(#( #task_arg_names, )* )
						},)*
						Task::__Ignore(_, _) => unreachable!(),
					}
				}

				#[allow(unused_variables)]
				fn weight(&self) -> #scrate::pallet_prelude::Weight {
					match self.clone() {
						#(#enum_ident::#task_fn_idents { #(#task_arg_names),* } => #task_weights,)*
						Task::__Ignore(_, _) => unreachable!(),
					}
				}
			}
		});
	}
}

/// Expands the [`TasksDef`] in the enclosing [`Def`], if present, and returns its tokens.
///
/// This modifies the underlying [`Def`] in addition to returning any tokens that were added.
pub fn expand_tasks_impl(def: &mut Def) -> TokenStream2 {
	let Some(tasks) = &mut def.tasks else { return quote!() };
	let ExpandedTasksDef { task_item_impl, task_trait_impl } = parse_quote!(#tasks);
	quote! {
		#task_item_impl
		#task_trait_impl
	}
}

/// Represents a fully-expanded [`TaskEnumDef`].
#[derive(Parse)]
pub struct ExpandedTaskEnum {
	pub item_enum: ItemEnum,
	pub debug_impl: ItemImpl,
}

/// Modifies a [`Def`] to expand the underlying [`TaskEnumDef`] if present, and also returns
/// its tokens. A blank [`TokenStream2`] is returned if no [`TaskEnumDef`] has been generated
/// or defined.
pub fn expand_task_enum(def: &mut Def) -> TokenStream2 {
	let Some(task_enum) = &mut def.task_enum else { return quote!() };
	let ExpandedTaskEnum { item_enum, debug_impl } = parse_quote!(#task_enum);
	quote! {
		#item_enum
		#debug_impl
	}
}

/// Modifies a [`Def`] to expand the underlying [`TasksDef`] and also generate a
/// [`TaskEnumDef`] if applicable. The tokens for these items are returned if they are created.
pub fn expand_tasks(def: &mut Def) -> TokenStream2 {
	if let Some(tasks_def) = &def.tasks {
		if def.task_enum.is_none() {
			def.task_enum = Some(TaskEnumDef::generate(
				&tasks_def,
				def.type_decl_bounded_generics(tasks_def.item_impl.span()),
				def.type_use_generics(tasks_def.item_impl.span()),
			));
		}
	}
	let tasks_extra_output = expand_tasks_impl(def);
	let task_enum_extra_output = expand_task_enum(def);
	quote! {
		#tasks_extra_output
		#task_enum_extra_output
	}
}
