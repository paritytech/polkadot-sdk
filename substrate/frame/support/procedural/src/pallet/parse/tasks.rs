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

use derive_syn_parse::Parse;
use proc_macro2::Span;
use quote::ToTokens;
use syn::{
	parse::ParseStream,
	parse2,
	spanned::Spanned,
	token::{Brace, Bracket, Paren, PathSep, Pound},
	Attribute, Error, Expr, Ident, ImplItemFn, Item, ItemImpl, LitInt, Result, Token,
};

#[cfg(test)]
macro_rules! assert_error_matches {
	($expr:expr, $reg:literal) => {
		match $expr {
			Ok(_) => panic!("Expected an `Error(..)`, but got Ok(..)"),
			Err(e) => {
				let error_message = e.to_string();
				let re = regex::Regex::new($reg).expect("Invalid regex pattern");
				assert!(
					re.is_match(&error_message),
					"Error message \"{}\" does not match the pattern \"{}\"",
					error_message,
					$reg
				);
			},
		}
	};
}

pub mod keywords {
	use syn::custom_keyword;

	custom_keyword!(tasks);
	custom_keyword!(task_list);
	custom_keyword!(task_condition);
	custom_keyword!(task_index);
	custom_keyword!(pallet);
}

pub struct TasksDef {
	normal_attrs: Vec<Attribute>,
	tasks_attr: PalletTasksAttr,
	tasks: Vec<TaskDef>,
}

impl syn::parse::Parse for TasksDef {
	fn parse(input: ParseStream) -> Result<Self> {
		let item_impl: ItemImpl = input.parse()?;
		let (tasks_attrs, normal_attrs): (Vec<_>, Vec<_>) =
			item_impl.attrs.into_iter().partition(|attr| {
				let mut path_segs = attr.path().segments.iter();
				let (Some(prefix), Some(suffix)) = (path_segs.next(), path_segs.next()) else {
					return false
				};
				prefix.ident == "pallet" && suffix.ident == "tasks"
			});
		let Some(tasks_attr) = tasks_attrs.first() else {
			return Err(Error::new(
				item_impl.impl_token.span(),
				"expected `#[pallet::tasks]` attribute",
			))
		};
		if let Some(extra_tasks_attr) = tasks_attrs.get(1) {
			return Err(Error::new(
				extra_tasks_attr.span(),
				"unexpected extra `#[pallet::tasks]` attribute",
			))
		}
		let tasks_attr = parse2::<PalletTasksAttr>(tasks_attr.to_token_stream())?;
		let tasks: Vec<TaskDef> = item_impl
			.items
			.into_iter()
			.map(|item| parse2::<TaskDef>(item.to_token_stream()))
			.collect::<Result<_>>()?;
		let mut task_indices = HashSet::<LitInt>::new();
		//for task in tasks {}
		Ok(TasksDef { normal_attrs, tasks_attr, tasks })
	}
}

impl TasksDef {
	pub fn try_from(_span: Span, _index: usize, _item: &mut Item) -> Result<Self> {
		todo!()
	}
}

pub type PalletTasksAttr = PalletTaskAttr<keywords::tasks>;

pub struct TaskDef {
	task_attrs: Vec<PalletTaskAttr<TaskAttrMeta>>,
	item: ImplItemFn,
}

impl syn::parse::Parse for TaskDef {
	fn parse(input: ParseStream) -> Result<Self> {
		let mut item = input.parse::<ImplItemFn>()?;
		// we only want to activate TaskAttrType parsing errors for tasks-related attributes,
		// so we filter them here
		let (task_attrs, normal_attrs) = item.attrs.into_iter().partition(|attr| {
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
		item.attrs = normal_attrs;
		let task_attrs: Vec<TaskAttr> = task_attrs
			.into_iter()
			.map(|attr| parse2(attr.to_token_stream()))
			.collect::<Result<_>>()?;

		Ok(TaskDef { task_attrs, item })
	}
}

#[derive(Parse, Debug)]
pub enum TaskAttrMeta {
	#[peek(keywords::task_list, name = "#[pallet::task_list(..)]")]
	TaskList(TaskListAttrMeta),
	#[peek(keywords::task_index, name = "#[pallet::task_index(..)")]
	TaskIndex(TaskIndexAttrMeta),
	#[peek(keywords::task_condition, name = "#[pallet::task_condition(..)")]
	TaskCondition(TaskConditionAttrMeta),
}

#[derive(Parse, Debug)]
pub struct TaskListAttrMeta {
	_tasks: keywords::task_list,
	#[paren]
	_paren: Paren,
	#[inside(_paren)]
	expr: Expr,
}

#[derive(Parse, Debug)]
pub struct TaskIndexAttrMeta {
	_task_index: keywords::task_index,
	#[paren]
	_paren: Paren,
	#[inside(_paren)]
	index: LitInt,
}

#[derive(Parse, Debug)]
pub struct TaskConditionAttrMeta {
	_condition: keywords::task_condition,
	#[paren]
	_paren: Paren,
	#[inside(_paren)]
	_pipe1: Token![|],
	#[inside(_paren)]
	_ident: Ident,
	#[inside(_paren)]
	_pipe2: Token![|],
	#[inside(_paren)]
	expr: Expr,
}

#[derive(Parse, Debug)]
pub struct PalletTaskAttr<T: syn::parse::Parse + core::fmt::Debug> {
	_pound: Pound,
	#[bracket]
	_bracket: Bracket,
	#[inside(_bracket)]
	_pallet: keywords::pallet,
	#[inside(_bracket)]
	_colons: PathSep,
	#[inside(_bracket)]
	attr: T,
}

pub type TaskAttr = PalletTaskAttr<TaskAttrMeta>;

#[cfg(test)]
use quote::quote;

#[test]
fn test_parse_pallet_task_list_() {
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
fn test_parse_pallet_task_index() {
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
fn test_parse_pallet_task_condition() {
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
fn test_parse_pallet_tasks_attr() {
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
