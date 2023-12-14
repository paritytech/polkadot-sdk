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

use super::{helper, InheritedCallWeightAttr};
use frame_support_procedural_tools::get_doc_literals;
use proc_macro2::Span;
use quote::ToTokens;
use std::collections::HashMap;
use syn::{spanned::Spanned, ExprClosure};

/// List of additional token to be used for parsing.
mod keyword {
	syn::custom_keyword!(Call);
	syn::custom_keyword!(OriginFor);
	syn::custom_keyword!(RuntimeOrigin);
	syn::custom_keyword!(weight);
	syn::custom_keyword!(call_index);
	syn::custom_keyword!(compact);
	syn::custom_keyword!(T);
	syn::custom_keyword!(pallet);
	syn::custom_keyword!(feeless_if);
}

/// Definition of dispatchables typically `impl<T: Config> Pallet<T> { ... }`
pub struct CallDef {
	/// The where_clause used.
	pub where_clause: Option<syn::WhereClause>,
	/// A set of usage of instance, must be check for consistency with trait.
	pub instances: Vec<helper::InstanceUsage>,
	/// The index of call item in pallet module.
	pub index: usize,
	/// Information on methods (used for expansion).
	pub methods: Vec<CallVariantDef>,
	/// The span of the pallet::call attribute.
	pub attr_span: proc_macro2::Span,
	/// Docs, specified on the impl Block.
	pub docs: Vec<syn::Expr>,
	/// The optional `weight` attribute on the `pallet::call`.
	pub inherited_call_weight: Option<InheritedCallWeightAttr>,
}

/// The weight of a call.
#[derive(Clone)]
pub enum CallWeightDef {
	/// Explicitly set on the call itself with `#[pallet::weight(…)]`. This value is used.
	Immediate(syn::Expr),

	/// The default value that should be set for dev-mode pallets. Usually zero.
	DevModeDefault,

	/// Inherits whatever value is configured on the pallet level.
	///
	/// The concrete value is not known at this point.
	Inherited,
}

/// Definition of dispatchable typically: `#[weight...] fn foo(origin .., param1: ...) -> ..`
#[derive(Clone)]
pub struct CallVariantDef {
	/// Function name.
	pub name: syn::Ident,
	/// Information on args: `(is_compact, name, type)`
	pub args: Vec<(bool, syn::Ident, Box<syn::Type>)>,
	/// Weight for the call.
	pub weight: CallWeightDef,
	/// Call index of the dispatchable.
	pub call_index: u8,
	/// Whether an explicit call index was specified.
	pub explicit_call_index: bool,
	/// Docs, used for metadata.
	pub docs: Vec<syn::Expr>,
	/// Attributes annotated at the top of the dispatchable function.
	pub attrs: Vec<syn::Attribute>,
	/// The `cfg` attributes.
	pub cfg_attrs: Vec<syn::Attribute>,
	/// The optional `feeless_if` attribute on the `pallet::call`.
	pub feeless_check: Option<syn::ExprClosure>,
}

/// Attributes for functions in call impl block.
pub enum FunctionAttr {
	/// Parse for `#[pallet::call_index(expr)]`
	CallIndex(u8),
	/// Parse for `#[pallet::weight(expr)]`
	Weight(syn::Expr),
	/// Parse for `#[pallet::feeless_if(expr)]`
	FeelessIf(Span, syn::ExprClosure),
}

impl syn::parse::Parse for FunctionAttr {
	fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
		input.parse::<syn::Token![#]>()?;
		let content;
		syn::bracketed!(content in input);
		content.parse::<keyword::pallet>()?;
		content.parse::<syn::Token![::]>()?;

		let lookahead = content.lookahead1();
		if lookahead.peek(keyword::weight) {
			content.parse::<keyword::weight>()?;
			let weight_content;
			syn::parenthesized!(weight_content in content);
			Ok(FunctionAttr::Weight(weight_content.parse::<syn::Expr>()?))
		} else if lookahead.peek(keyword::call_index) {
			content.parse::<keyword::call_index>()?;
			let call_index_content;
			syn::parenthesized!(call_index_content in content);
			let index = call_index_content.parse::<syn::LitInt>()?;
			if !index.suffix().is_empty() {
				let msg = "Number literal must not have a suffix";
				return Err(syn::Error::new(index.span(), msg))
			}
			Ok(FunctionAttr::CallIndex(index.base10_parse()?))
		} else if lookahead.peek(keyword::feeless_if) {
			content.parse::<keyword::feeless_if>()?;
			let closure_content;
			syn::parenthesized!(closure_content in content);
			Ok(FunctionAttr::FeelessIf(
				closure_content.span(),
				closure_content.parse::<syn::ExprClosure>().map_err(|e| {
					let msg = "Invalid feeless_if attribute: expected a closure";
					let mut err = syn::Error::new(closure_content.span(), msg);
					err.combine(e);
					err
				})?,
			))
		} else {
			Err(lookahead.error())
		}
	}
}

/// Attribute for arguments in function in call impl block.
/// Parse for `#[pallet::compact]|
pub struct ArgAttrIsCompact;

impl syn::parse::Parse for ArgAttrIsCompact {
	fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
		input.parse::<syn::Token![#]>()?;
		let content;
		syn::bracketed!(content in input);
		content.parse::<keyword::pallet>()?;
		content.parse::<syn::Token![::]>()?;

		content.parse::<keyword::compact>()?;
		Ok(ArgAttrIsCompact)
	}
}

/// Check the syntax is `OriginFor<T>`, `&OriginFor<T>` or `T::RuntimeOrigin`.
pub fn check_dispatchable_first_arg_type(ty: &syn::Type, is_ref: bool) -> syn::Result<()> {
	pub struct CheckOriginFor(bool);
	impl syn::parse::Parse for CheckOriginFor {
		fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
			let is_ref = input.parse::<syn::Token![&]>().is_ok();
			input.parse::<keyword::OriginFor>()?;
			input.parse::<syn::Token![<]>()?;
			input.parse::<keyword::T>()?;
			input.parse::<syn::Token![>]>()?;

			Ok(Self(is_ref))
		}
	}

	pub struct CheckRuntimeOrigin;
	impl syn::parse::Parse for CheckRuntimeOrigin {
		fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
			input.parse::<keyword::T>()?;
			input.parse::<syn::Token![::]>()?;
			input.parse::<keyword::RuntimeOrigin>()?;

			Ok(Self)
		}
	}

	let result_origin_for = syn::parse2::<CheckOriginFor>(ty.to_token_stream());
	let result_runtime_origin = syn::parse2::<CheckRuntimeOrigin>(ty.to_token_stream());
	return match (result_origin_for, result_runtime_origin) {
		(Ok(CheckOriginFor(has_ref)), _) if is_ref == has_ref => Ok(()),
		(_, Ok(_)) => Ok(()),
		(_, _) => {
			let msg = if is_ref {
				"Invalid type: expected `&OriginFor<T>`"
			} else {
				"Invalid type: expected `OriginFor<T>` or `T::RuntimeOrigin`"
			};
			return Err(syn::Error::new(ty.span(), msg))
		},
	}
}

impl CallDef {
	pub fn try_from(
		attr_span: proc_macro2::Span,
		index: usize,
		item: &mut syn::Item,
		dev_mode: bool,
		inherited_call_weight: Option<InheritedCallWeightAttr>,
	) -> syn::Result<Self> {
		let item_impl = if let syn::Item::Impl(item) = item {
			item
		} else {
			return Err(syn::Error::new(item.span(), "Invalid pallet::call, expected item impl"))
		};

		let instances = vec![
			helper::check_impl_gen(&item_impl.generics, item_impl.impl_token.span())?,
			helper::check_pallet_struct_usage(&item_impl.self_ty)?,
		];

		if let Some((_, _, for_)) = item_impl.trait_ {
			let msg = "Invalid pallet::call, expected no trait ident as in \
				`impl<..> Pallet<..> { .. }`";
			return Err(syn::Error::new(for_.span(), msg))
		}

		let mut methods = vec![];
		let mut indices = HashMap::new();
		let mut last_index: Option<u8> = None;
		for item in &mut item_impl.items {
			if let syn::ImplItem::Fn(method) = item {
				if !matches!(method.vis, syn::Visibility::Public(_)) {
					let msg = "Invalid pallet::call, dispatchable function must be public: \
						`pub fn`";

					let span = match method.vis {
						syn::Visibility::Inherited => method.sig.span(),
						_ => method.vis.span(),
					};

					return Err(syn::Error::new(span, msg))
				}

				match method.sig.inputs.first() {
					None => {
						let msg = "Invalid pallet::call, must have at least origin arg";
						return Err(syn::Error::new(method.sig.span(), msg))
					},
					Some(syn::FnArg::Receiver(_)) => {
						let msg = "Invalid pallet::call, first argument must be a typed argument, \
							e.g. `origin: OriginFor<T>`";
						return Err(syn::Error::new(method.sig.span(), msg))
					},
					Some(syn::FnArg::Typed(arg)) => {
						check_dispatchable_first_arg_type(&arg.ty, false)?;
					},
				}

				if let syn::ReturnType::Type(_, type_) = &method.sig.output {
					helper::check_pallet_call_return_type(type_)?;
				} else {
					let msg = "Invalid pallet::call, require return type \
						DispatchResultWithPostInfo";
					return Err(syn::Error::new(method.sig.span(), msg))
				}

				let cfg_attrs: Vec<syn::Attribute> = helper::get_item_cfg_attrs(&method.attrs);
				let mut call_idx_attrs = vec![];
				let mut weight_attrs = vec![];
				let mut feeless_attrs = vec![];
				for attr in helper::take_item_pallet_attrs(&mut method.attrs)?.into_iter() {
					match attr {
						FunctionAttr::CallIndex(_) => {
							call_idx_attrs.push(attr);
						},
						FunctionAttr::Weight(_) => {
							weight_attrs.push(attr);
						},
						FunctionAttr::FeelessIf(span, _) => {
							feeless_attrs.push((span, attr));
						},
					}
				}

				if weight_attrs.is_empty() && dev_mode {
					// inject a default O(1) weight when dev mode is enabled and no weight has
					// been specified on the call
					let empty_weight: syn::Expr = syn::parse_quote!(0);
					weight_attrs.push(FunctionAttr::Weight(empty_weight));
				}

				let weight = match weight_attrs.len() {
					0 if inherited_call_weight.is_some() => CallWeightDef::Inherited,
					0 if dev_mode => CallWeightDef::DevModeDefault,
					0 => return Err(syn::Error::new(
						method.sig.span(),
						"A pallet::call requires either a concrete `#[pallet::weight($expr)]` or an
						inherited weight from the `#[pallet:call(weight($type))]` attribute, but
						none were given.",
					)),
					1 => match weight_attrs.pop().unwrap() {
						FunctionAttr::Weight(w) => CallWeightDef::Immediate(w),
						_ => unreachable!("checked during creation of the let binding"),
					},
					_ => {
						let msg = "Invalid pallet::call, too many weight attributes given";
						return Err(syn::Error::new(method.sig.span(), msg))
					},
				};

				if call_idx_attrs.len() > 1 {
					let msg = "Invalid pallet::call, too many call_index attributes given";
					return Err(syn::Error::new(method.sig.span(), msg))
				}
				let call_index = call_idx_attrs.pop().map(|attr| match attr {
					FunctionAttr::CallIndex(idx) => idx,
					_ => unreachable!("checked during creation of the let binding"),
				});
				let explicit_call_index = call_index.is_some();

				let final_index = match call_index {
					Some(i) => i,
					None =>
						last_index.map_or(Some(0), |idx| idx.checked_add(1)).ok_or_else(|| {
							let msg = "Call index doesn't fit into u8, index is 256";
							syn::Error::new(method.sig.span(), msg)
						})?,
				};
				last_index = Some(final_index);

				if let Some(used_fn) = indices.insert(final_index, method.sig.ident.clone()) {
					let msg = format!(
						"Call indices are conflicting: Both functions {} and {} are at index {}",
						used_fn, method.sig.ident, final_index,
					);
					let mut err = syn::Error::new(used_fn.span(), &msg);
					err.combine(syn::Error::new(method.sig.ident.span(), msg));
					return Err(err)
				}

				let mut args = vec![];
				for arg in method.sig.inputs.iter_mut().skip(1) {
					let arg = if let syn::FnArg::Typed(arg) = arg {
						arg
					} else {
						unreachable!("Only first argument can be receiver");
					};

					let arg_attrs: Vec<ArgAttrIsCompact> =
						helper::take_item_pallet_attrs(&mut arg.attrs)?;

					if arg_attrs.len() > 1 {
						let msg = "Invalid pallet::call, argument has too many attributes";
						return Err(syn::Error::new(arg.span(), msg))
					}

					let arg_ident = if let syn::Pat::Ident(pat) = &*arg.pat {
						pat.ident.clone()
					} else {
						let msg = "Invalid pallet::call, argument must be ident";
						return Err(syn::Error::new(arg.pat.span(), msg))
					};

					args.push((!arg_attrs.is_empty(), arg_ident, arg.ty.clone()));
				}

				let docs = get_doc_literals(&method.attrs);

				if feeless_attrs.len() > 1 {
					let msg = "Invalid pallet::call, there can only be one feeless_if attribute";
					return Err(syn::Error::new(feeless_attrs[1].0, msg))
				}
				let feeless_check: Option<ExprClosure> =
					feeless_attrs.pop().map(|(_, attr)| match attr {
						FunctionAttr::FeelessIf(_, closure) => closure,
						_ => unreachable!("checked during creation of the let binding"),
					});

				if let Some(ref feeless_check) = feeless_check {
					if feeless_check.inputs.len() != args.len() + 1 {
						let msg = "Invalid pallet::call, feeless_if closure must have same \
							number of arguments as the dispatchable function";
						return Err(syn::Error::new(feeless_check.span(), msg))
					}

					match feeless_check.inputs.first() {
						None => {
							let msg = "Invalid pallet::call, feeless_if closure must have at least origin arg";
							return Err(syn::Error::new(feeless_check.span(), msg))
						},
						Some(syn::Pat::Type(arg)) => {
							check_dispatchable_first_arg_type(&arg.ty, true)?;
						},
						_ => {
							let msg = "Invalid pallet::call, feeless_if closure first argument must be a typed argument, \
								e.g. `origin: OriginFor<T>`";
							return Err(syn::Error::new(feeless_check.span(), msg))
						},
					}

					for (feeless_arg, arg) in feeless_check.inputs.iter().skip(1).zip(args.iter()) {
						let feeless_arg_type =
							if let syn::Pat::Type(syn::PatType { ty, .. }) = feeless_arg.clone() {
								if let syn::Type::Reference(pat) = *ty {
									pat.elem.clone()
								} else {
									let msg = "Invalid pallet::call, feeless_if closure argument must be a reference";
									return Err(syn::Error::new(ty.span(), msg))
								}
							} else {
								let msg = "Invalid pallet::call, feeless_if closure argument must be a type ascription pattern";
								return Err(syn::Error::new(feeless_arg.span(), msg))
							};

						if feeless_arg_type != arg.2 {
							let msg =
								"Invalid pallet::call, feeless_if closure argument must have \
								a reference to the same type as the dispatchable function argument";
							return Err(syn::Error::new(feeless_arg.span(), msg))
						}
					}

					let valid_return = match &feeless_check.output {
						syn::ReturnType::Type(_, type_) => match *(type_.clone()) {
							syn::Type::Path(syn::TypePath { path, .. }) => path.is_ident("bool"),
							_ => false,
						},
						_ => false,
					};
					if !valid_return {
						let msg = "Invalid pallet::call, feeless_if closure must return `bool`";
						return Err(syn::Error::new(feeless_check.output.span(), msg))
					}
				}

				methods.push(CallVariantDef {
					name: method.sig.ident.clone(),
					weight,
					call_index: final_index,
					explicit_call_index,
					args,
					docs,
					attrs: method.attrs.clone(),
					cfg_attrs,
					feeless_check,
				});
			} else {
				let msg = "Invalid pallet::call, only method accepted";
				return Err(syn::Error::new(item.span(), msg))
			}
		}

		Ok(Self {
			index,
			attr_span,
			instances,
			methods,
			where_clause: item_impl.generics.where_clause.clone(),
			docs: get_doc_literals(&item_impl.attrs),
			inherited_call_weight,
		})
	}
}
