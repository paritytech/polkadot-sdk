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

//! Home of the parsing code for the Tasks API

use std::collections::HashSet;

#[cfg(test)]
use crate::assert_parse_error_matches;

#[cfg(test)]
use crate::pallet::parse::tests::simulate_manifest_dir;

use derive_syn_parse::Parse;
use frame_support_procedural_tools::generate_access_from_frame_or_crate;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, ToTokens};
use syn::{
	parse::ParseStream,
	parse2,
	spanned::Spanned,
	token::{Bracket, Paren, PathSep, Pound},
	Attribute, Error, Expr, Ident, ImplItem, ImplItemFn, ItemEnum, ItemImpl, LitInt, Path,
	PathArguments, Result, TypePath,
};

pub mod keywords {
	use syn::custom_keyword;

	custom_keyword!(tasks_experimental);
	custom_keyword!(task_enum);
	custom_keyword!(task_list);
	custom_keyword!(task_condition);
	custom_keyword!(task_index);
	custom_keyword!(task_weight);
	custom_keyword!(pallet);
}

/// Represents the `#[pallet::tasks_experimental]` attribute and its attached item. Also includes
/// metadata about the linked [`TaskEnumDef`] if applicable.
#[derive(Clone, Debug)]
pub struct TasksDef {
	pub tasks_attr: Option<PalletTasksAttr>,
	pub tasks: Vec<TaskDef>,
	pub item_impl: ItemImpl,
	/// Path to `frame_support`
	pub scrate: Path,
	pub enum_ident: Ident,
	pub enum_arguments: PathArguments,
}

impl syn::parse::Parse for TasksDef {
	fn parse(input: ParseStream) -> Result<Self> {
		let item_impl: ItemImpl = input.parse()?;
		let (tasks_attrs, normal_attrs) = partition_tasks_attrs(&item_impl);
		let tasks_attr = match tasks_attrs.first() {
			Some(attr) => Some(parse2::<PalletTasksAttr>(attr.to_token_stream())?),
			None => None,
		};
		if let Some(extra_tasks_attr) = tasks_attrs.get(1) {
			return Err(Error::new(
				extra_tasks_attr.span(),
				"unexpected extra `#[pallet::tasks_experimental]` attribute",
			))
		}
		let tasks: Vec<TaskDef> = if tasks_attr.is_some() {
			item_impl
				.items
				.clone()
				.into_iter()
				.filter(|impl_item| matches!(impl_item, ImplItem::Fn(_)))
				.map(|item| parse2::<TaskDef>(item.to_token_stream()))
				.collect::<Result<_>>()?
		} else {
			Vec::new()
		};
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

		// we require the path on the impl to be a TypePath
		let enum_path = parse2::<TypePath>(item_impl.self_ty.to_token_stream())?;
		let segments = enum_path.path.segments.iter().collect::<Vec<_>>();
		let (Some(last_seg), None) = (segments.get(0), segments.get(1)) else {
			return Err(Error::new(
				enum_path.span(),
				"if specified manually, the task enum must be defined locally in this \
				pallet and cannot be a re-export",
			))
		};
		let enum_ident = last_seg.ident.clone();
		let enum_arguments = last_seg.arguments.clone();

		// We do this here because it would be improper to do something fallible like this at
		// the expansion phase. Fallible stuff should happen during parsing.
		let scrate = generate_access_from_frame_or_crate("frame-support")?;

		Ok(TasksDef { tasks_attr, item_impl, tasks, scrate, enum_ident, enum_arguments })
	}
}

/// Parsing for a `#[pallet::tasks_experimental]` attr.
pub type PalletTasksAttr = PalletTaskAttr<keywords::tasks_experimental>;

/// Parsing for any of the attributes that can be used within a `#[pallet::tasks_experimental]`
/// [`ItemImpl`].
pub type TaskAttr = PalletTaskAttr<TaskAttrMeta>;

/// Parsing for a `#[pallet::task_index]` attr.
pub type TaskIndexAttr = PalletTaskAttr<TaskIndexAttrMeta>;

/// Parsing for a `#[pallet::task_condition]` attr.
pub type TaskConditionAttr = PalletTaskAttr<TaskConditionAttrMeta>;

/// Parsing for a `#[pallet::task_list]` attr.
pub type TaskListAttr = PalletTaskAttr<TaskListAttrMeta>;

/// Parsing for a `#[pallet::task_weight]` attr.
pub type TaskWeightAttr = PalletTaskAttr<TaskWeightAttrMeta>;

/// Parsing for a `#[pallet:task_enum]` attr.
pub type PalletTaskEnumAttr = PalletTaskAttr<keywords::task_enum>;

/// Parsing for a manually-specified (or auto-generated) task enum, optionally including the
/// attached `#[pallet::task_enum]` attribute.
#[derive(Clone, Debug)]
pub struct TaskEnumDef {
	pub attr: Option<PalletTaskEnumAttr>,
	pub item_enum: ItemEnum,
	pub scrate: Path,
	pub type_use_generics: TokenStream2,
}

impl syn::parse::Parse for TaskEnumDef {
	fn parse(input: ParseStream) -> Result<Self> {
		let mut item_enum = input.parse::<ItemEnum>()?;
		let attr = extract_pallet_attr(&mut item_enum)?;
		let attr = match attr {
			Some(attr) => Some(parse2(attr)?),
			None => None,
		};

		// We do this here because it would be improper to do something fallible like this at
		// the expansion phase. Fallible stuff should happen during parsing.
		let scrate = generate_access_from_frame_or_crate("frame-support")?;

		let type_use_generics = quote!(T);

		Ok(TaskEnumDef { attr, item_enum, scrate, type_use_generics })
	}
}

/// Represents an individual tasks within a [`TasksDef`].
#[derive(Debug, Clone)]
pub struct TaskDef {
	pub index_attr: TaskIndexAttr,
	pub condition_attr: TaskConditionAttr,
	pub list_attr: TaskListAttr,
	pub weight_attr: TaskWeightAttr,
	pub normal_attrs: Vec<Attribute>,
	pub item: ImplItemFn,
	pub arg_names: Vec<Ident>,
}

impl syn::parse::Parse for TaskDef {
	fn parse(input: ParseStream) -> Result<Self> {
		let item = input.parse::<ImplItemFn>()?;
		// we only want to activate TaskAttrType parsing errors for tasks-related attributes,
		// so we filter them here
		let (task_attrs, normal_attrs) = partition_task_attrs(&item);

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

		let Some(weight_attr) = task_attrs
			.iter()
			.find(|attr| matches!(attr.meta, TaskAttrMeta::TaskWeight(_)))
			.cloned()
		else {
			return Err(Error::new(
				item.sig.ident.span(),
				"missing `#[pallet::task_weight(..)]` attribute",
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

		let mut arg_names = vec![];
		for input in item.sig.inputs.iter() {
			match input {
				syn::FnArg::Typed(pat_type) => match &*pat_type.pat {
					syn::Pat::Ident(ident) => arg_names.push(ident.ident.clone()),
					_ => return Err(Error::new(input.span(), "unexpected pattern type")),
				},
				_ => return Err(Error::new(input.span(), "unexpected function argument type")),
			}
		}

		let index_attr = index_attr.try_into().expect("we check the type above; QED");
		let condition_attr = condition_attr.try_into().expect("we check the type above; QED");
		let list_attr = list_attr.try_into().expect("we check the type above; QED");
		let weight_attr = weight_attr.try_into().expect("we check the type above; QED");

		Ok(TaskDef {
			index_attr,
			condition_attr,
			list_attr,
			weight_attr,
			normal_attrs,
			item,
			arg_names,
		})
	}
}

/// The contents of a [`TasksDef`]-related attribute.
#[derive(Parse, Debug, Clone)]
pub enum TaskAttrMeta {
	#[peek(keywords::task_list, name = "#[pallet::task_list(..)]")]
	TaskList(TaskListAttrMeta),
	#[peek(keywords::task_index, name = "#[pallet::task_index(..)")]
	TaskIndex(TaskIndexAttrMeta),
	#[peek(keywords::task_condition, name = "#[pallet::task_condition(..)")]
	TaskCondition(TaskConditionAttrMeta),
	#[peek(keywords::task_weight, name = "#[pallet::task_weight(..)")]
	TaskWeight(TaskWeightAttrMeta),
}

/// The contents of a `#[pallet::task_list]` attribute.
#[derive(Parse, Debug, Clone)]
pub struct TaskListAttrMeta {
	pub task_list: keywords::task_list,
	#[paren]
	_paren: Paren,
	#[inside(_paren)]
	pub expr: Expr,
}

/// The contents of a `#[pallet::task_index]` attribute.
#[derive(Parse, Debug, Clone)]
pub struct TaskIndexAttrMeta {
	pub task_index: keywords::task_index,
	#[paren]
	_paren: Paren,
	#[inside(_paren)]
	pub index: LitInt,
}

/// The contents of a `#[pallet::task_condition]` attribute.
#[derive(Parse, Debug, Clone)]
pub struct TaskConditionAttrMeta {
	pub task_condition: keywords::task_condition,
	#[paren]
	_paren: Paren,
	#[inside(_paren)]
	pub expr: Expr,
}

/// The contents of a `#[pallet::task_weight]` attribute.
#[derive(Parse, Debug, Clone)]
pub struct TaskWeightAttrMeta {
	pub task_weight: keywords::task_weight,
	#[paren]
	_paren: Paren,
	#[inside(_paren)]
	pub expr: Expr,
}

/// The contents of a `#[pallet::task]` attribute.
#[derive(Parse, Debug, Clone)]
pub struct PalletTaskAttr<T: syn::parse::Parse + core::fmt::Debug + ToTokens> {
	pub pound: Pound,
	#[bracket]
	_bracket: Bracket,
	#[inside(_bracket)]
	pub pallet: keywords::pallet,
	#[inside(_bracket)]
	pub colons: PathSep,
	#[inside(_bracket)]
	pub meta: T,
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
		let expr = &self.expr;
		tokens.extend(quote!(#task_condition(#expr)));
	}
}

impl ToTokens for TaskWeightAttrMeta {
	fn to_tokens(&self, tokens: &mut TokenStream2) {
		let task_weight = self.task_weight;
		let expr = &self.expr;
		tokens.extend(quote!(#task_weight(#expr)));
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
			TaskAttrMeta::TaskWeight(weight) => tokens.extend(weight.to_token_stream()),
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

impl TryFrom<PalletTaskAttr<TaskAttrMeta>> for TaskWeightAttr {
	type Error = syn::Error;

	fn try_from(value: PalletTaskAttr<TaskAttrMeta>) -> Result<Self> {
		let pound = value.pound;
		let pallet = value.pallet;
		let colons = value.colons;
		match value.meta {
			TaskAttrMeta::TaskWeight(meta) => parse2(quote!(#pound[#pallet #colons #meta])),
			_ =>
				return Err(Error::new(
					value.span(),
					format!("`{:?}` cannot be converted to a `TaskWeightAttr`", value.meta),
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

fn extract_pallet_attr(item_enum: &mut ItemEnum) -> Result<Option<TokenStream2>> {
	let mut duplicate = None;
	let mut attr = None;
	item_enum.attrs = item_enum
		.attrs
		.iter()
		.filter(|found_attr| {
			let segs = found_attr
				.path()
				.segments
				.iter()
				.map(|seg| seg.ident.clone())
				.collect::<Vec<_>>();
			let (Some(seg1), Some(_), None) = (segs.get(0), segs.get(1), segs.get(2)) else {
				return true
			};
			if seg1 != "pallet" {
				return true
			}
			if attr.is_some() {
				duplicate = Some(found_attr.span());
			}
			attr = Some(found_attr.to_token_stream());
			false
		})
		.cloned()
		.collect();
	if let Some(span) = duplicate {
		return Err(Error::new(span, "only one `#[pallet::_]` attribute is supported on this item"))
	}
	Ok(attr)
}

fn partition_tasks_attrs(item_impl: &ItemImpl) -> (Vec<syn::Attribute>, Vec<syn::Attribute>) {
	item_impl.attrs.clone().into_iter().partition(|attr| {
		let mut path_segs = attr.path().segments.iter();
		let (Some(prefix), Some(suffix), None) =
			(path_segs.next(), path_segs.next(), path_segs.next())
		else {
			return false
		};
		prefix.ident == "pallet" && suffix.ident == "tasks_experimental"
	})
}

fn partition_task_attrs(item: &ImplItemFn) -> (Vec<syn::Attribute>, Vec<syn::Attribute>) {
	item.attrs.clone().into_iter().partition(|attr| {
		let mut path_segs = attr.path().segments.iter();
		let (Some(prefix), Some(suffix)) = (path_segs.next(), path_segs.next()) else {
			return false
		};
		// N.B: the `PartialEq` impl between `Ident` and `&str` is more efficient than
		// parsing and makes no stack or heap allocations
		prefix.ident == "pallet" &&
			(suffix.ident == "tasks_experimental" ||
				suffix.ident == "task_list" ||
				suffix.ident == "task_condition" ||
				suffix.ident == "task_weight" ||
				suffix.ident == "task_index")
	})
}

#[test]
fn test_parse_task_list_() {
	parse2::<TaskAttr>(quote!(#[pallet::task_list(Something::iter())])).unwrap();
	parse2::<TaskAttr>(quote!(#[pallet::task_list(Numbers::<T, I>::iter_keys())])).unwrap();
	parse2::<TaskAttr>(quote!(#[pallet::task_list(iter())])).unwrap();
	assert_parse_error_matches!(
		parse2::<TaskAttr>(quote!(#[pallet::task_list()])),
		"expected an expression"
	);
	assert_parse_error_matches!(
		parse2::<TaskAttr>(quote!(#[pallet::task_list])),
		"expected parentheses"
	);
}

#[test]
fn test_parse_task_index() {
	parse2::<TaskAttr>(quote!(#[pallet::task_index(3)])).unwrap();
	parse2::<TaskAttr>(quote!(#[pallet::task_index(0)])).unwrap();
	parse2::<TaskAttr>(quote!(#[pallet::task_index(17)])).unwrap();
	assert_parse_error_matches!(
		parse2::<TaskAttr>(quote!(#[pallet::task_index])),
		"expected parentheses"
	);
	assert_parse_error_matches!(
		parse2::<TaskAttr>(quote!(#[pallet::task_index("hey")])),
		"expected integer literal"
	);
	assert_parse_error_matches!(
		parse2::<TaskAttr>(quote!(#[pallet::task_index(0.3)])),
		"expected integer literal"
	);
}

#[test]
fn test_parse_task_condition() {
	parse2::<TaskAttr>(quote!(#[pallet::task_condition(|x| x.is_some())])).unwrap();
	parse2::<TaskAttr>(quote!(#[pallet::task_condition(|_x| some_expr())])).unwrap();
	parse2::<TaskAttr>(quote!(#[pallet::task_condition(|| some_expr())])).unwrap();
	parse2::<TaskAttr>(quote!(#[pallet::task_condition(some_expr())])).unwrap();
}

#[test]
fn test_parse_tasks_attr() {
	parse2::<PalletTasksAttr>(quote!(#[pallet::tasks_experimental])).unwrap();
	assert_parse_error_matches!(
		parse2::<PalletTasksAttr>(quote!(#[pallet::taskss])),
		"expected `tasks_experimental`"
	);
	assert_parse_error_matches!(
		parse2::<PalletTasksAttr>(quote!(#[pallet::tasks_])),
		"expected `tasks_experimental`"
	);
	assert_parse_error_matches!(
		parse2::<PalletTasksAttr>(quote!(#[pal::tasks])),
		"expected `pallet`"
	);
	assert_parse_error_matches!(
		parse2::<PalletTasksAttr>(quote!(#[pallet::tasks_experimental()])),
		"unexpected token"
	);
}

#[test]
fn test_parse_tasks_def_basic() {
	simulate_manifest_dir("../../examples/basic", || {
		let parsed = parse2::<TasksDef>(quote! {
			#[pallet::tasks_experimental]
			impl<T: Config<I>, I: 'static> Pallet<T, I> {
				/// Add a pair of numbers into the totals and remove them.
				#[pallet::task_list(Numbers::<T, I>::iter_keys())]
				#[pallet::task_condition(|i| Numbers::<T, I>::contains_key(i))]
				#[pallet::task_index(0)]
				#[pallet::task_weight(0)]
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
		assert_eq!(parsed.tasks.len(), 1);
	});
}

#[test]
fn test_parse_tasks_def_basic_increment_decrement() {
	simulate_manifest_dir("../../examples/basic", || {
		let parsed = parse2::<TasksDef>(quote! {
			#[pallet::tasks_experimental]
			impl<T: Config<I>, I: 'static> Pallet<T, I> {
				/// Get the value and check if it can be incremented
				#[pallet::task_index(0)]
				#[pallet::task_condition(|| {
					let value = Value::<T>::get().unwrap();
					value < 255
				})]
				#[pallet::task_list(Vec::<Task<T>>::new())]
				#[pallet::task_weight(0)]
				fn increment() -> DispatchResult {
					let value = Value::<T>::get().unwrap_or_default();
					if value >= 255 {
						Err(Error::<T>::ValueOverflow.into())
					} else {
						let new_val = value.checked_add(1).ok_or(Error::<T>::ValueOverflow)?;
						Value::<T>::put(new_val);
						Pallet::<T>::deposit_event(Event::Incremented { new_val });
						Ok(())
					}
				}

				// Get the value and check if it can be decremented
				#[pallet::task_index(1)]
				#[pallet::task_condition(|| {
					let value = Value::<T>::get().unwrap();
					value > 0
				})]
				#[pallet::task_list(Vec::<Task<T>>::new())]
				#[pallet::task_weight(0)]
				fn decrement() -> DispatchResult {
					let value = Value::<T>::get().unwrap_or_default();
					if value == 0 {
						Err(Error::<T>::ValueUnderflow.into())
					} else {
						let new_val = value.checked_sub(1).ok_or(Error::<T>::ValueUnderflow)?;
						Value::<T>::put(new_val);
						Pallet::<T>::deposit_event(Event::Decremented { new_val });
						Ok(())
					}
				}
			}
		})
		.unwrap();
		assert_eq!(parsed.tasks.len(), 2);
	});
}

#[test]
fn test_parse_tasks_def_duplicate_index() {
	simulate_manifest_dir("../../examples/basic", || {
		assert_parse_error_matches!(
			parse2::<TasksDef>(quote! {
				#[pallet::tasks_experimental]
				impl<T: Config<I>, I: 'static> Pallet<T, I> {
					#[pallet::task_list(Something::iter())]
					#[pallet::task_condition(|i| i % 2 == 0)]
					#[pallet::task_index(0)]
					#[pallet::task_weight(0)]
					pub fn foo(i: u32) -> DispatchResult {
						Ok(())
					}

					#[pallet::task_list(Numbers::<T, I>::iter_keys())]
					#[pallet::task_condition(|i| Numbers::<T, I>::contains_key(i))]
					#[pallet::task_index(0)]
					#[pallet::task_weight(0)]
					pub fn bar(i: u32) -> DispatchResult {
						Ok(())
					}
				}
			}),
			"duplicate task index `0`"
		);
	});
}

#[test]
fn test_parse_tasks_def_missing_task_list() {
	simulate_manifest_dir("../../examples/basic", || {
		assert_parse_error_matches!(
			parse2::<TasksDef>(quote! {
				#[pallet::tasks_experimental]
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
	});
}

#[test]
fn test_parse_tasks_def_missing_task_condition() {
	simulate_manifest_dir("../../examples/basic", || {
		assert_parse_error_matches!(
			parse2::<TasksDef>(quote! {
				#[pallet::tasks_experimental]
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
	});
}

#[test]
fn test_parse_tasks_def_missing_task_index() {
	simulate_manifest_dir("../../examples/basic", || {
		assert_parse_error_matches!(
			parse2::<TasksDef>(quote! {
				#[pallet::tasks_experimental]
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
	});
}

#[test]
fn test_parse_tasks_def_missing_task_weight() {
	simulate_manifest_dir("../../examples/basic", || {
		assert_parse_error_matches!(
			parse2::<TasksDef>(quote! {
				#[pallet::tasks_experimental]
				impl<T: Config<I>, I: 'static> Pallet<T, I> {
					#[pallet::task_condition(|i| i % 2 == 0)]
					#[pallet::task_list(Something::iter())]
					#[pallet::task_index(0)]
					pub fn foo(i: u32) -> DispatchResult {
						Ok(())
					}
				}
			}),
			r"missing `#\[pallet::task_weight\(\.\.\)\]`"
		);
	});
}

#[test]
fn test_parse_tasks_def_unexpected_extra_task_list_attr() {
	simulate_manifest_dir("../../examples/basic", || {
		assert_parse_error_matches!(
			parse2::<TasksDef>(quote! {
				#[pallet::tasks_experimental]
				impl<T: Config<I>, I: 'static> Pallet<T, I> {
					#[pallet::task_condition(|i| i % 2 == 0)]
					#[pallet::task_index(0)]
					#[pallet::task_weight(0)]
					#[pallet::task_list(Something::iter())]
					#[pallet::task_list(SomethingElse::iter())]
					pub fn foo(i: u32) -> DispatchResult {
						Ok(())
					}
				}
			}),
			r"unexpected extra `#\[pallet::task_list\(\.\.\)\]`"
		);
	});
}

#[test]
fn test_parse_tasks_def_unexpected_extra_task_condition_attr() {
	simulate_manifest_dir("../../examples/basic", || {
		assert_parse_error_matches!(
			parse2::<TasksDef>(quote! {
				#[pallet::tasks_experimental]
				impl<T: Config<I>, I: 'static> Pallet<T, I> {
					#[pallet::task_condition(|i| i % 2 == 0)]
					#[pallet::task_condition(|i| i % 4 == 0)]
					#[pallet::task_index(0)]
					#[pallet::task_list(Something::iter())]
					#[pallet::task_weight(0)]
					pub fn foo(i: u32) -> DispatchResult {
						Ok(())
					}
				}
			}),
			r"unexpected extra `#\[pallet::task_condition\(\.\.\)\]`"
		);
	});
}

#[test]
fn test_parse_tasks_def_unexpected_extra_task_index_attr() {
	simulate_manifest_dir("../../examples/basic", || {
		assert_parse_error_matches!(
			parse2::<TasksDef>(quote! {
				#[pallet::tasks_experimental]
				impl<T: Config<I>, I: 'static> Pallet<T, I> {
					#[pallet::task_condition(|i| i % 2 == 0)]
					#[pallet::task_index(0)]
					#[pallet::task_index(0)]
					#[pallet::task_list(Something::iter())]
					#[pallet::task_weight(0)]
					pub fn foo(i: u32) -> DispatchResult {
						Ok(())
					}
				}
			}),
			r"unexpected extra `#\[pallet::task_index\(\.\.\)\]`"
		);
	});
}

#[test]
fn test_parse_tasks_def_extra_tasks_attribute() {
	simulate_manifest_dir("../../examples/basic", || {
		assert_parse_error_matches!(
			parse2::<TasksDef>(quote! {
				#[pallet::tasks_experimental]
				#[pallet::tasks_experimental]
				impl<T: Config<I>, I: 'static> Pallet<T, I> {}
			}),
			r"unexpected extra `#\[pallet::tasks_experimental\]` attribute"
		);
	});
}

#[test]
fn test_parse_task_enum_def_basic() {
	simulate_manifest_dir("../../examples/basic", || {
		parse2::<TaskEnumDef>(quote! {
			#[pallet::task_enum]
			pub enum Task<T: Config> {
				Increment,
				Decrement,
			}
		})
		.unwrap();
	});
}

#[test]
fn test_parse_task_enum_def_non_task_name() {
	simulate_manifest_dir("../../examples/basic", || {
		parse2::<TaskEnumDef>(quote! {
			#[pallet::task_enum]
			pub enum Something {
				Foo
			}
		})
		.unwrap();
	});
}

#[test]
fn test_parse_task_enum_def_missing_attr_allowed() {
	simulate_manifest_dir("../../examples/basic", || {
		parse2::<TaskEnumDef>(quote! {
			pub enum Task<T: Config> {
				Increment,
				Decrement,
			}
		})
		.unwrap();
	});
}

#[test]
fn test_parse_task_enum_def_missing_attr_alternate_name_allowed() {
	simulate_manifest_dir("../../examples/basic", || {
		parse2::<TaskEnumDef>(quote! {
			pub enum Foo {
				Red,
			}
		})
		.unwrap();
	});
}

#[test]
fn test_parse_task_enum_def_wrong_attr() {
	simulate_manifest_dir("../../examples/basic", || {
		assert_parse_error_matches!(
			parse2::<TaskEnumDef>(quote! {
				#[pallet::something]
				pub enum Task<T: Config> {
					Increment,
					Decrement,
				}
			}),
			"expected `task_enum`"
		);
	});
}

#[test]
fn test_parse_task_enum_def_wrong_item() {
	simulate_manifest_dir("../../examples/basic", || {
		assert_parse_error_matches!(
			parse2::<TaskEnumDef>(quote! {
				#[pallet::task_enum]
				pub struct Something;
			}),
			"expected `enum`"
		);
	});
}
