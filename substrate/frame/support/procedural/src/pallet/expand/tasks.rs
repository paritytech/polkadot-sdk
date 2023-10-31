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

use crate::pallet::{parse::tasks::*, Def};
use derive_syn_parse::Parse;
use inflector::Inflector;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote, ToTokens};
use syn::{parse_quote, spanned::Spanned, Item, ItemEnum, ItemImpl};

impl TaskEnumDef {
	pub fn generate(
		tasks: &TasksDef,
		type_decl_bounded_generics: TokenStream2,
		type_use_generics: TokenStream2,
	) -> Self {
		let variants = match tasks.tasks_attr.is_some() {
			true => tasks
				.tasks
				.iter()
				.map(|task| {
					let ident = &task.item.sig.ident;
					let ident = format_ident!("{}", ident.to_string().to_class_case());

					let args = task.item.sig.inputs.iter().collect::<Vec<_>>();

					if args.is_empty() {
						quote!(#ident)
					} else {
						quote!(#ident {
							#(#args),*
						})
					}
				})
				.collect::<Vec<_>>(),
			false => Vec::new(),
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
					Clone,
					PartialEq,
					Eq,
					#scrate::pallet_prelude::Encode,
					#scrate::pallet_prelude::Decode,
					#scrate::pallet_prelude::TypeInfo,
				)]
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

impl ToTokens for TasksDef {
	fn to_tokens(&self, tokens: &mut TokenStream2) {
		let scrate = &self.scrate;
		let enum_ident = &self.enum_ident;
		let enum_arguments = &self.enum_arguments;
		let enum_use = quote!(#enum_ident #enum_arguments);

		let task_fn_idents = self
			.tasks
			.iter()
			.map(|task| format_ident!("{}", &task.item.sig.ident.to_string().to_class_case()))
			.collect::<Vec<_>>();
		let task_indices = self.tasks.iter().map(|task| &task.index_attr.meta.index);
		let task_conditions = self.tasks.iter().map(|task| &task.condition_attr.meta.expr);
		let task_iters = self.tasks.iter().map(|task| &task.list_attr.meta.expr);
		let task_fn_blocks = self.tasks.iter().map(|task| &task.item.block);
		let task_arg_names = self
			.tasks
			.iter()
			.map(|task| {
				task.item
					.sig
					.inputs
					.iter()
					.map(|input| match input {
						// Todo: This should be checked in the parsing stage.
						syn::FnArg::Typed(pat_type) => match &*pat_type.pat {
							syn::Pat::Ident(ident) => quote!(#ident),
							_ => panic!("unexpected pattern type"),
						},
						_ => panic!("unexpected function argument type"),
					})
					.collect::<Vec<_>>()
			})
			.collect::<Vec<_>>();

		let sp_std = quote!(#scrate::__private::sp_std);
		let impl_generics = &self.item_impl.generics;
		tokens.extend(quote! {
			impl #impl_generics #scrate::traits::Task for #enum_use
			where
				T: #scrate::pallet_prelude::TypeInfo,
			{
				type Enumeration = #sp_std::vec::IntoIter<#enum_use>;

				fn iter() -> Self::Enumeration {
					let mut all_tasks = #sp_std::vec![];
					#(all_tasks.extend(#task_iters.map(|i| #enum_ident::#task_fn_idents { i }).collect::<#sp_std::vec::Vec<_>>());)*
					all_tasks.into_iter()
				}

				fn task_index(&self) -> u32 {
					match self {
						#(#enum_ident::#task_fn_idents { .. } => #task_indices),*,
						Task::__Ignore(_, _) => unreachable!(),
					}
				}

				fn is_valid(&self) -> bool {
					match self {
						#(#enum_ident::#task_fn_idents { #(#task_arg_names),* } => (#task_conditions)(#(#task_arg_names),* )),*,
						Task::__Ignore(_, _) => unreachable!(),
					}
				}

				fn run(&self) -> Result<(), #scrate::pallet_prelude::DispatchError> {
					match self.clone() {
						#(#enum_ident::#task_fn_idents { #(#task_arg_names),* } => #task_fn_blocks),*,
						Task::__Ignore(_, _) => unreachable!(),
					}
				}

				fn weight(&self) -> #scrate::pallet_prelude::Weight {
					#scrate::pallet_prelude::Weight::default()
				}
			}
		});
	}
}

pub fn expand_tasks_impl(def: &mut Def) -> TokenStream2 {
	let Some(tasks) = &mut def.tasks else { return quote!() };
	let output: ItemImpl = parse_quote!(#tasks);
	// output.pretty_print();
	let Some(content) = &mut def.item.content else { return quote!() };
	for item in content.1.iter_mut() {
		let Item::Impl(item_impl) = item else { continue };
		let Some(trait_) = &item_impl.trait_ else { continue };
		let Some(last_seg) = trait_.1.segments.last() else { continue };
		if last_seg.ident == "Task" {
			*item_impl = output;
			break
		}
	}
	quote!()
}

#[derive(Parse)]
pub struct ExpandedTaskEnum {
	pub item_enum: ItemEnum,
	pub debug_impl: ItemImpl,
}

pub fn expand_task_enum(def: &mut Def) -> TokenStream2 {
	let Some(task_enum) = &mut def.task_enum else { return quote!() };
	let ExpandedTaskEnum { item_enum, debug_impl } = parse_quote!(#task_enum);
	// item_enum.pretty_print();
	// debug_impl.pretty_print();
	quote! {
		#item_enum
		#debug_impl
	}
}

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
