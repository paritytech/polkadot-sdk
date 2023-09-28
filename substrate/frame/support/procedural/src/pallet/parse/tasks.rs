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

use std::collections::HashSet;

#[cfg(test)]
use crate::assert_error_matches;

use derive_syn_parse::Parse;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{quote, ToTokens};
use syn::{
	parse::ParseStream,
	parse2,
	spanned::Spanned,
	token::{Bracket, Paren, PathSep, Pound},
	Attribute, Error, Expr, Ident, ImplItemFn, ItemEnum, ItemImpl, LitInt, Result, Token,
};

pub mod keywords {
	use syn::custom_keyword;

	custom_keyword!(tasks);
	custom_keyword!(task_enum);
	custom_keyword!(task_list);
	custom_keyword!(task_condition);
	custom_keyword!(task_index);
	custom_keyword!(pallet);
}

pub struct TasksDef {
	tasks_attr: Option<PalletTasksAttr>,
	tasks: Vec<TaskDef>,
	item_impl: ItemImpl,
}

impl syn::parse::Parse for TasksDef {
	fn parse(input: ParseStream) -> Result<Self> {
		let item_impl: ItemImpl = input.parse()?;
		let (tasks_attrs, normal_attrs): (Vec<_>, Vec<_>) =
			item_impl.attrs.clone().into_iter().partition(|attr| {
				let mut path_segs = attr.path().segments.iter();
				let (Some(prefix), Some(suffix)) = (path_segs.next(), path_segs.next()) else {
					return false
				};
				prefix.ident == "pallet" && suffix.ident == "tasks"
			});
		let tasks_attr = match tasks_attrs.first() {
			Some(attr) => Some(parse2::<PalletTasksAttr>(attr.to_token_stream())?),
			None => None,
		};
		if let Some(extra_tasks_attr) = tasks_attrs.get(1) {
			return Err(Error::new(
				extra_tasks_attr.span(),
				"unexpected extra `#[pallet::tasks]` attribute",
			))
		}
		let tasks: Vec<TaskDef> = item_impl
			.items
			.clone()
			.into_iter()
			.map(|item| parse2::<TaskDef>(item.to_token_stream()))
			.collect::<Result<_>>()?;
		let mut task_indices = HashSet::<LitInt>::new();
		for task in tasks.iter() {
			let task_index = &task.index_attr.meta.index;
			if !task_indices.insert(task_index.clone()) {
				return Err(Error::new(
					task_index.span(),
					format!("duplicate task index `{}`", task_index),
				))
			}
		}
		let mut item_impl = item_impl;
		item_impl.attrs = normal_attrs;
		Ok(TasksDef { tasks_attr, item_impl, tasks })
	}
}

pub type PalletTasksAttr = PalletTaskAttr<keywords::tasks>;
pub type TaskAttr = PalletTaskAttr<TaskAttrMeta>;
pub type TaskIndexAttr = PalletTaskAttr<TaskIndexAttrMeta>;
pub type TaskConditionAttr = PalletTaskAttr<TaskConditionAttrMeta>;
pub type TaskListAttr = PalletTaskAttr<TaskListAttrMeta>;
pub type PalletTaskEnumAttr = PalletTaskAttr<keywords::task_enum>;

#[derive(Clone, Debug)]
pub struct TaskEnumDef {
	attr: Option<PalletTaskEnumAttr>,
	item_enum: ItemEnum,
}

impl syn::parse::Parse for TaskEnumDef {
	fn parse(input: ParseStream) -> Result<Self> {
		let item_enum = input.parse::<ItemEnum>()?;
		let mut attr = None;
		for found_attr in &item_enum.attrs {
			let segs = found_attr
				.path()
				.segments
				.iter()
				.map(|seg| seg.ident.clone())
				.collect::<Vec<_>>();
			let (Some(seg1), Some(_), None) = (segs.get(0), segs.get(1), segs.get(2)) else {
				continue
			};
			if seg1 != "pallet" {
				continue
			}
			if attr.is_some() {
				return Err(Error::new(
					found_attr.span(),
					"only one `#[pallet::_]` attribute is supported on this item",
				))
			}
			attr = Some(parse2(found_attr.to_token_stream())?);
		}
		Ok(TaskEnumDef { attr, item_enum })
	}
}

#[derive(Debug, Clone)]
pub struct TaskDef {
	index_attr: TaskIndexAttr,
	condition_attr: TaskConditionAttr,
	list_attr: TaskListAttr,
	normal_attrs: Vec<Attribute>,
}

impl syn::parse::Parse for TaskDef {
	fn parse(input: ParseStream) -> Result<Self> {
		let item = input.parse::<ImplItemFn>()?;
		// we only want to activate TaskAttrType parsing errors for tasks-related attributes,
		// so we filter them here
		let (task_attrs, normal_attrs): (Vec<_>, Vec<_>) =
			item.attrs.into_iter().partition(|attr| {
				let mut path_segs = attr.path().segments.iter();
				let (Some(prefix), Some(suffix)) = (path_segs.next(), path_segs.next()) else {
					return false
				};
				// N.B: the `PartialEq` impl between `Ident` and `&str` is more efficient than
				// parsing and makes no stack or heap allocations
				prefix.ident == "pallet" &&
					(suffix.ident == "tasks" ||
						suffix.ident == "task_list" ||
						suffix.ident == "task_condition" ||
						suffix.ident == "task_index")
			});

		let task_attrs: Vec<TaskAttr> = task_attrs
			.into_iter()
			.map(|attr| parse2(attr.to_token_stream()))
			.collect::<Result<_>>()?;

		let Some(index_attr) = task_attrs
			.iter()
			.find(|attr| matches!(attr.meta, TaskAttrMeta::TaskIndex(_)))
			.cloned()
		else {
			return Err(Error::new(
				item.sig.ident.span(),
				"missing `#[pallet::task_index(..)]` attribute",
			))
		};

		let Some(condition_attr) = task_attrs
			.iter()
			.find(|attr| matches!(attr.meta, TaskAttrMeta::TaskCondition(_)))
			.cloned()
		else {
			return Err(Error::new(
				item.sig.ident.span(),
				"missing `#[pallet::task_condition(..)]` attribute",
			))
		};

		let Some(list_attr) = task_attrs
			.iter()
			.find(|attr| matches!(attr.meta, TaskAttrMeta::TaskList(_)))
			.cloned()
		else {
			return Err(Error::new(
				item.sig.ident.span(),
				"missing `#[pallet::task_list(..)]` attribute",
			))
		};

		if let Some(duplicate) = task_attrs
			.iter()
			.filter(|attr| matches!(attr.meta, TaskAttrMeta::TaskCondition(_)))
			.collect::<Vec<_>>()
			.get(1)
		{
			return Err(Error::new(
				duplicate.span(),
				"unexpected extra `#[pallet::task_condition(..)]` attribute",
			))
		}

		if let Some(duplicate) = task_attrs
			.iter()
			.filter(|attr| matches!(attr.meta, TaskAttrMeta::TaskList(_)))
			.collect::<Vec<_>>()
			.get(1)
		{
			return Err(Error::new(
				duplicate.span(),
				"unexpected extra `#[pallet::task_list(..)]` attribute",
			))
		}

		if let Some(duplicate) = task_attrs
			.iter()
			.filter(|attr| matches!(attr.meta, TaskAttrMeta::TaskIndex(_)))
			.collect::<Vec<_>>()
			.get(1)
		{
			return Err(Error::new(
				duplicate.span(),
				"unexpected extra `#[pallet::task_index(..)]` attribute",
			))
		}

		let index_attr = index_attr.try_into().expect("we check the type above; QED");
		let condition_attr = condition_attr.try_into().expect("we check the type above; QED");
		let list_attr = list_attr.try_into().expect("we check the type above; QED");

		Ok(TaskDef { index_attr, condition_attr, list_attr, normal_attrs })
	}
}

#[derive(Parse, Debug, Clone)]
pub enum TaskAttrMeta {
	#[peek(keywords::task_list, name = "#[pallet::task_list(..)]")]
	TaskList(TaskListAttrMeta),
	#[peek(keywords::task_index, name = "#[pallet::task_index(..)")]
	TaskIndex(TaskIndexAttrMeta),
	#[peek(keywords::task_condition, name = "#[pallet::task_condition(..)")]
	TaskCondition(TaskConditionAttrMeta),
}

#[derive(Parse, Debug, Clone)]
pub struct TaskListAttrMeta {
	task_list: keywords::task_list,
	#[paren]
	_paren: Paren,
	#[inside(_paren)]
	expr: Expr,
}

#[derive(Parse, Debug, Clone)]
pub struct TaskIndexAttrMeta {
	task_index: keywords::task_index,
	#[paren]
	_paren: Paren,
	#[inside(_paren)]
	index: LitInt,
}

#[derive(Parse, Debug, Clone)]
pub struct TaskConditionAttrMeta {
	task_condition: keywords::task_condition,
	#[paren]
	_paren: Paren,
	#[inside(_paren)]
	pipe1: Token![|],
	#[inside(_paren)]
	ident: Ident,
	#[inside(_paren)]
	pipe2: Token![|],
	#[inside(_paren)]
	expr: Expr,
}

#[derive(Parse, Debug, Clone)]
pub struct PalletTaskAttr<T: syn::parse::Parse + core::fmt::Debug + ToTokens> {
	pound: Pound,
	#[bracket]
	_bracket: Bracket,
	#[inside(_bracket)]
	pallet: keywords::pallet,
	#[inside(_bracket)]
	colons: PathSep,
	#[inside(_bracket)]
	meta: T,
}

impl ToTokens for TaskListAttrMeta {
	fn to_tokens(&self, tokens: &mut TokenStream2) {
		let task_list = self.task_list;
		let expr = &self.expr;
		tokens.extend(quote!(#task_list(#expr)));
	}
}

impl ToTokens for TaskConditionAttrMeta {
	fn to_tokens(&self, tokens: &mut TokenStream2) {
		let task_condition = self.task_condition;
		let pipe1 = self.pipe1;
		let ident = &self.ident;
		let pipe2 = self.pipe2;
		let expr = &self.expr;
		tokens.extend(quote!(#task_condition(#pipe1 #ident #pipe2 #expr)));
	}
}

impl ToTokens for TaskIndexAttrMeta {
	fn to_tokens(&self, tokens: &mut TokenStream2) {
		let task_index = self.task_index;
		let index = &self.index;
		tokens.extend(quote!(#task_index(#index)))
	}
}

impl ToTokens for TaskAttrMeta {
	fn to_tokens(&self, tokens: &mut TokenStream2) {
		match self {
			TaskAttrMeta::TaskList(list) => tokens.extend(list.to_token_stream()),
			TaskAttrMeta::TaskIndex(index) => tokens.extend(index.to_token_stream()),
			TaskAttrMeta::TaskCondition(condition) => tokens.extend(condition.to_token_stream()),
		}
	}
}

impl<T: syn::parse::Parse + core::fmt::Debug + ToTokens> ToTokens for PalletTaskAttr<T> {
	fn to_tokens(&self, tokens: &mut TokenStream2) {
		let pound = self.pound;
		let pallet = self.pallet;
		let colons = self.colons;
		let meta = &self.meta;
		tokens.extend(quote!(#pound[#pallet #colons #meta]));
	}
}

impl TryFrom<PalletTaskAttr<TaskAttrMeta>> for TaskIndexAttr {
	type Error = syn::Error;

	fn try_from(value: PalletTaskAttr<TaskAttrMeta>) -> Result<Self> {
		let pound = value.pound;
		let pallet = value.pallet;
		let colons = value.colons;
		match value.meta {
			TaskAttrMeta::TaskIndex(meta) => parse2(quote!(#pound[#pallet #colons #meta])),
			_ =>
				return Err(Error::new(
					value.span(),
					format!("`{:?}` cannot be converted to a `TaskIndexAttr`", value.meta),
				)),
		}
	}
}

impl TryFrom<PalletTaskAttr<TaskAttrMeta>> for TaskConditionAttr {
	type Error = syn::Error;

	fn try_from(value: PalletTaskAttr<TaskAttrMeta>) -> Result<Self> {
		let pound = value.pound;
		let pallet = value.pallet;
		let colons = value.colons;
		match value.meta {
			TaskAttrMeta::TaskCondition(meta) => parse2(quote!(#pound[#pallet #colons #meta])),
			_ =>
				return Err(Error::new(
					value.span(),
					format!("`{:?}` cannot be converted to a `TaskConditionAttr`", value.meta),
				)),
		}
	}
}

impl TryFrom<PalletTaskAttr<TaskAttrMeta>> for TaskListAttr {
	type Error = syn::Error;

	fn try_from(value: PalletTaskAttr<TaskAttrMeta>) -> Result<Self> {
		let pound = value.pound;
		let pallet = value.pallet;
		let colons = value.colons;
		match value.meta {
			TaskAttrMeta::TaskList(meta) => parse2(quote!(#pound[#pallet #colons #meta])),
			_ =>
				return Err(Error::new(
					value.span(),
					format!("`{:?}` cannot be converted to a `TaskListAttr`", value.meta),
				)),
		}
	}
}

#[test]
fn test_parse_task_list_() {
	parse2::<TaskAttr>(quote!(#[pallet::task_list(Something::iter())])).unwrap();
	parse2::<TaskAttr>(quote!(#[pallet::task_list(Numbers::<T, I>::iter_keys())])).unwrap();
	parse2::<TaskAttr>(quote!(#[pallet::task_list(iter())])).unwrap();
	assert_error_matches!(
		parse2::<TaskAttr>(quote!(#[pallet::task_list()])),
		"expected an expression"
	);
	assert_error_matches!(parse2::<TaskAttr>(quote!(#[pallet::task_list])), "expected parentheses");
}

#[test]
fn test_parse_task_index() {
	parse2::<TaskAttr>(quote!(#[pallet::task_index(3)])).unwrap();
	parse2::<TaskAttr>(quote!(#[pallet::task_index(0)])).unwrap();
	parse2::<TaskAttr>(quote!(#[pallet::task_index(17)])).unwrap();
	assert_error_matches!(
		parse2::<TaskAttr>(quote!(#[pallet::task_index])),
		"expected parentheses"
	);
	assert_error_matches!(
		parse2::<TaskAttr>(quote!(#[pallet::task_index("hey")])),
		"expected integer literal"
	);
	assert_error_matches!(
		parse2::<TaskAttr>(quote!(#[pallet::task_index(0.3)])),
		"expected integer literal"
	);
}

#[test]
fn test_parse_task_condition() {
	parse2::<TaskAttr>(quote!(#[pallet::task_condition(|x| x.is_some())])).unwrap();
	parse2::<TaskAttr>(quote!(#[pallet::task_condition(|_x| some_expr())])).unwrap();
	assert_error_matches!(
		parse2::<TaskAttr>(quote!(#[pallet::task_condition(x.is_some())])),
		"expected `|`"
	);
	assert_error_matches!(
		parse2::<TaskAttr>(quote!(#[pallet::task_condition(|| something())])),
		"expected identifier"
	);
}

#[test]
fn test_parse_tasks_attr() {
	parse2::<PalletTasksAttr>(quote!(#[pallet::tasks])).unwrap();
	assert_error_matches!(parse2::<PalletTasksAttr>(quote!(#[pallet::taskss])), "expected `tasks`");
	assert_error_matches!(parse2::<PalletTasksAttr>(quote!(#[pallet::tasks_])), "expected `tasks`");
	assert_error_matches!(parse2::<PalletTasksAttr>(quote!(#[pal::tasks])), "expected `pallet`");
	assert_error_matches!(
		parse2::<PalletTasksAttr>(quote!(#[pallet::tasks()])),
		"unexpected token"
	);
}

#[test]
fn test_parse_tasks_def_basic() {
	parse2::<TasksDef>(quote! {
		#[pallet::tasks]
		impl<T: Config<I>, I: 'static> Pallet<T, I> {
			/// Add a pair of numbers into the totals and remove them.
			#[pallet::task_list(Numbers::<T, I>::iter_keys())]
			#[pallet::task_condition(|i| Numbers::<T, I>::contains_key(i))]
			#[pallet::task_index(0)]
			pub fn add_number_into_total(i: u32) -> DispatchResult {
				let v = Numbers::<T, I>::take(i).ok_or(Error::<T, I>::NotFound)?;
				Total::<T, I>::mutate(|(total_keys, total_values)| {
					*total_keys += i;
					*total_values += v;
				});
				Ok(())
			}
		}
	})
	.unwrap();
}

#[test]
fn test_parse_tasks_def_duplicate_index() {
	assert_error_matches!(
		parse2::<TasksDef>(quote! {
			#[pallet::tasks]
			impl<T: Config<I>, I: 'static> Pallet<T, I> {
				#[pallet::task_list(Something::iter())]
				#[pallet::task_condition(|i| i % 2 == 0)]
				#[pallet::task_index(0)]
				pub fn foo(i: u32) -> DispatchResult {
					Ok(())
				}

				#[pallet::task_list(Numbers::<T, I>::iter_keys())]
				#[pallet::task_condition(|i| Numbers::<T, I>::contains_key(i))]
				#[pallet::task_index(0)]
				pub fn bar(i: u32) -> DispatchResult {
					Ok(())
				}
			}
		}),
		"duplicate task index `0`"
	);
}

#[test]
fn test_parse_tasks_def_missing_task_list() {
	assert_error_matches!(
		parse2::<TasksDef>(quote! {
			#[pallet::tasks]
			impl<T: Config<I>, I: 'static> Pallet<T, I> {
				#[pallet::task_condition(|i| i % 2 == 0)]
				#[pallet::task_index(0)]
				pub fn foo(i: u32) -> DispatchResult {
					Ok(())
				}
			}
		}),
		r"missing `#\[pallet::task_list\(\.\.\)\]`"
	);
}

#[test]
fn test_parse_tasks_def_missing_task_condition() {
	assert_error_matches!(
		parse2::<TasksDef>(quote! {
			#[pallet::tasks]
			impl<T: Config<I>, I: 'static> Pallet<T, I> {
				#[pallet::task_list(Something::iter())]
				#[pallet::task_index(0)]
				pub fn foo(i: u32) -> DispatchResult {
					Ok(())
				}
			}
		}),
		r"missing `#\[pallet::task_condition\(\.\.\)\]`"
	);
}

#[test]
fn test_parse_tasks_def_missing_task_index() {
	assert_error_matches!(
		parse2::<TasksDef>(quote! {
			#[pallet::tasks]
			impl<T: Config<I>, I: 'static> Pallet<T, I> {
				#[pallet::task_condition(|i| i % 2 == 0)]
				#[pallet::task_list(Something::iter())]
				pub fn foo(i: u32) -> DispatchResult {
					Ok(())
				}
			}
		}),
		r"missing `#\[pallet::task_index\(\.\.\)\]`"
	);
}

#[test]
fn test_parse_tasks_def_unexpected_extra_task_list_attr() {
	assert_error_matches!(
		parse2::<TasksDef>(quote! {
			#[pallet::tasks]
			impl<T: Config<I>, I: 'static> Pallet<T, I> {
				#[pallet::task_condition(|i| i % 2 == 0)]
				#[pallet::task_index(0)]
				#[pallet::task_list(Something::iter())]
				#[pallet::task_list(SomethingElse::iter())]
				pub fn foo(i: u32) -> DispatchResult {
					Ok(())
				}
			}
		}),
		r"unexpected extra `#\[pallet::task_list\(\.\.\)\]`"
	);
}

#[test]
fn test_parse_tasks_def_unexpected_extra_task_condition_attr() {
	assert_error_matches!(
		parse2::<TasksDef>(quote! {
			#[pallet::tasks]
			impl<T: Config<I>, I: 'static> Pallet<T, I> {
				#[pallet::task_condition(|i| i % 2 == 0)]
				#[pallet::task_condition(|i| i % 4 == 0)]
				#[pallet::task_index(0)]
				#[pallet::task_list(Something::iter())]
				pub fn foo(i: u32) -> DispatchResult {
					Ok(())
				}
			}
		}),
		r"unexpected extra `#\[pallet::task_condition\(\.\.\)\]`"
	);
}

#[test]
fn test_parse_tasks_def_unexpected_extra_task_index_attr() {
	assert_error_matches!(
		parse2::<TasksDef>(quote! {
			#[pallet::tasks]
			impl<T: Config<I>, I: 'static> Pallet<T, I> {
				#[pallet::task_condition(|i| i % 2 == 0)]
				#[pallet::task_index(0)]
				#[pallet::task_index(0)]
				#[pallet::task_list(Something::iter())]
				pub fn foo(i: u32) -> DispatchResult {
					Ok(())
				}
			}
		}),
		r"unexpected extra `#\[pallet::task_index\(\.\.\)\]`"
	);
}

#[test]
fn test_parse_tasks_def_extra_tasks_attribute() {
	assert_error_matches!(
		parse2::<TasksDef>(quote! {
			#[pallet::tasks]
			#[pallet::tasks]
			impl<T: Config<I>, I: 'static> Pallet<T, I> {}
		}),
		r"unexpected extra `#\[pallet::tasks\]` attribute"
	);
}

#[test]
fn test_parse_task_enum_def_basic() {
	parse2::<TaskEnumDef>(quote! {
		#[pallet::task_enum]
		pub enum Task<T: Config> {
			Increment,
			Decrement,
		}
	})
	.unwrap();
}

#[test]
fn test_parse_task_enum_def_non_task_name() {
	parse2::<TaskEnumDef>(quote! {
		#[pallet::task_enum]
		pub enum Something {
			Foo
		}
	})
	.unwrap();
}

#[test]
fn test_parse_task_enum_def_missing_attr_allowed() {
	parse2::<TaskEnumDef>(quote! {
		pub enum Task<T: Config> {
			Increment,
			Decrement,
		}
	})
	.unwrap();
}

#[test]
fn test_parse_task_enum_def_missing_attr_alternate_name_allowed() {
	parse2::<TaskEnumDef>(quote! {
		pub enum Foo {
			Red,
		}
	})
	.unwrap();
}

#[test]
fn test_parse_task_enum_def_wrong_attr() {
	assert_error_matches!(
		parse2::<TaskEnumDef>(quote! {
			#[pallet::something]
			pub enum Task<T: Config> {
				Increment,
				Decrement,
			}
		}),
		"expected `task_enum`"
	)
}

#[test]
fn test_parse_task_enum_def_wrong_item() {
	assert_error_matches!(
		parse2::<TaskEnumDef>(quote! {
			#[pallet::task_enum]
			pub struct Something;
		}),
		"expected `enum`"
	)
}
