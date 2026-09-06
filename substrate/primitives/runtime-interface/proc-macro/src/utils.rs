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

//! Util function used by this crate.

use proc_macro2::{Span, TokenStream};

use syn::{
	parse::Parse, parse_quote, spanned::Spanned, token, Error, FnArg, Ident, ItemTrait, LitInt,
	Pat, PatType, Result, Signature, TraitItem, TraitItemFn, Type,
};

use proc_macro_crate::{crate_name, FoundCrate};

use std::{
	collections::{btree_map::Entry, BTreeMap},
	env,
};

use quote::quote;

use inflector::Inflector;

mod attributes {
	syn::custom_keyword!(register_only);
}

/// The first ABI epoch: the original, pre-RFC-145 ABI using host-side allocation.
pub const ABI_EPOCH_LEGACY: u32 = 1;
/// The second ABI epoch: the RFC-145 ABI using runtime-side allocation.
pub const ABI_EPOCH_RFC145: u32 = 2;

/// Returns the `#[cfg]` attribute gating items that belong to the given ABI epoch, or `None`
/// for epoch 1 items, which are always compiled in.
pub fn abi_epoch_cfg(epoch: u32) -> Option<syn::Attribute> {
	(epoch >= ABI_EPOCH_RFC145).then(|| parse_quote!( #[cfg(rfc145)] ))
}

/// Returns the `#[cfg]` attribute gating items that must only be compiled when everything
/// above the first ABI epoch is disabled.
pub fn abi_epoch_negative_cfg() -> syn::Attribute {
	parse_quote!( #[cfg(not(rfc145))] )
}

/// Parses and strips an `#[abi_epoch(N)]` attribute, if present.
///
/// When the attribute denotes a gated epoch, the corresponding `#[cfg]` attribute is pushed
/// into the item's attributes in its place, so that all the code generation paths that
/// propagate `#[cfg]` attributes gate the generated items automatically.
fn extract_abi_epoch(item: &mut TraitItemFn) -> Result<Option<u32>> {
	let mut epoch = None;
	for attr in &item.attrs {
		if attr.path().is_ident("abi_epoch") {
			if epoch.is_some() {
				return Err(Error::new(attr.span(), "Duplicated `abi_epoch` attribute"));
			}
			let version: LitInt = attr.parse_args()?;
			let version = version.base10_parse::<u32>()?;
			if !(ABI_EPOCH_LEGACY..=ABI_EPOCH_RFC145).contains(&version) {
				return Err(Error::new(attr.span(), "Unknown ABI epoch"));
			}
			epoch = Some(version);
		}
	}
	if epoch.is_some() {
		item.attrs.retain(|attr| !attr.path().is_ident("abi_epoch"));
	}
	if let Some(cfg) = epoch.and_then(abi_epoch_cfg) {
		item.attrs.push(cfg);
	}
	Ok(epoch)
}

/// A concrete, specific version of a runtime interface function.
pub struct RuntimeInterfaceFunction {
	item: TraitItemFn,
	should_trap_on_return: bool,
	is_raw_api: bool,
	register_only: bool,
	abi_epoch: u32,
	declared_cfg_attrs: String,
}

impl std::ops::Deref for RuntimeInterfaceFunction {
	type Target = TraitItemFn;
	fn deref(&self) -> &Self::Target {
		&self.item
	}
}

impl RuntimeInterfaceFunction {
	fn new(item: &TraitItemFn, register_only: bool) -> Result<Self> {
		let mut item = item.clone();
		let mut should_trap_on_return = false;
		let mut is_raw_api = false;

		item.attrs.retain(|attr| {
			if attr.path().is_ident("trap_on_return") {
				should_trap_on_return = true;
				false
			} else if attr.path().is_ident("raw_api") {
				is_raw_api = true;
				false
			} else {
				true
			}
		});

		if should_trap_on_return && !matches!(item.sig.output, syn::ReturnType::Default) {
			return Err(Error::new(
				item.sig.ident.span(),
				"Methods marked as #[trap_on_return] cannot return anything",
			));
		}

		let declared_cfg_attrs = cfg_attrs_string(&item.attrs);
		let abi_epoch = extract_abi_epoch(&mut item)?.unwrap_or(ABI_EPOCH_LEGACY);

		if register_only && abi_epoch > ABI_EPOCH_LEGACY {
			return Err(Error::new(
				item.sig.ident.span(),
				"`register_only` doesn't make sense for versions of gated ABI epochs: they \
				 are only compiled in when the epoch is enabled, in which case they are meant \
				 to be used",
			));
		}

		Ok(Self {
			item,
			should_trap_on_return,
			is_raw_api,
			register_only,
			abi_epoch,
			declared_cfg_attrs,
		})
	}

	pub fn should_trap_on_return(&self) -> bool {
		self.should_trap_on_return
	}

	pub fn is_raw_api(&self) -> bool {
		self.is_raw_api
	}

	pub fn is_register_only(&self) -> bool {
		self.register_only
	}

	pub fn abi_epoch(&self) -> u32 {
		self.abi_epoch
	}
}

/// Returns the `#[cfg]` attributes of the given attribute list, stringified for comparison.
fn cfg_attrs_string(attrs: &[syn::Attribute]) -> String {
	use quote::ToTokens;
	attrs
		.iter()
		.filter(|attr| attr.path().is_ident("cfg"))
		.map(|attr| attr.to_token_stream().to_string())
		.collect::<Vec<_>>()
		.join(" ")
}

/// Runtime interface function with all associated versions of this function.
struct RuntimeInterfaceFunctionSet {
	latest_version_to_call: Option<u32>,
	versions: BTreeMap<u32, RuntimeInterfaceFunction>,
}

impl RuntimeInterfaceFunctionSet {
	fn new(version: VersionAttribute, trait_item: &TraitItemFn) -> Result<Self> {
		Ok(Self {
			latest_version_to_call: version.is_callable().then_some(version.version),
			versions: BTreeMap::from([(
				version.version,
				RuntimeInterfaceFunction::new(trait_item, !version.is_callable())?,
			)]),
		})
	}

	/// Returns the latest version of this runtime interface function plus the actual function
	/// implementation.
	///
	/// This isn't required to be the latest version, because a runtime interface function can be
	/// annotated with `register_only` to ensure that the host exposes the host function but it
	/// isn't used when compiling the runtime.
	pub fn latest_version_to_call(&self) -> Option<(u32, &RuntimeInterfaceFunction)> {
		self.latest_version_to_call.map(|v| {
			(
			v,
			self.versions.get(&v).expect(
				"If latest_version_to_call has a value, the key with this value is in the versions; qed",
			),
		)
		})
	}

	/// Returns the latest callable (non-`register_only`) version of this function that belongs
	/// to the first ABI epoch, if any.
	fn latest_legacy_version_to_call(&self) -> Option<(u32, &RuntimeInterfaceFunction)> {
		self.versions
			.iter()
			.rev()
			.find(|(_, item)| !item.is_register_only() && item.abi_epoch() == ABI_EPOCH_LEGACY)
			.map(|(v, item)| (*v, item))
	}

	/// Returns the version the bare function must call when the gated ABI epochs are compiled
	/// out, if it differs from [`Self::latest_version_to_call`].
	///
	/// This is `Some` only for functions that have versions in a gated ABI epoch on top of
	/// callable first-epoch versions: without the gated epochs the bare function falls back to
	/// the latest first-epoch version.
	pub fn legacy_version_to_call(&self) -> Option<(u32, &RuntimeInterfaceFunction)> {
		let (latest, item) = self.latest_version_to_call()?;
		(item.abi_epoch() > ABI_EPOCH_LEGACY)
			.then(|| self.latest_legacy_version_to_call())
			.flatten()
			.filter(|(legacy, _)| *legacy != latest)
	}

	/// Add a different version of the function.
	fn add_version(&mut self, version: VersionAttribute, trait_item: &TraitItemFn) -> Result<()> {
		if let Some(existing_item) = self.versions.get(&version.version) {
			let mut err = Error::new(trait_item.span(), "Duplicated version attribute");
			err.combine(Error::new(
				existing_item.span(),
				"Previous version with the same number defined here",
			));

			return Err(err);
		}

		self.versions.insert(
			version.version,
			RuntimeInterfaceFunction::new(trait_item, !version.is_callable())?,
		);
		if self.latest_version_to_call.map_or(true, |v| v < version.version) &&
			version.is_callable()
		{
			self.latest_version_to_call = Some(version.version);
		}

		Ok(())
	}
}

/// A `#[wrapper]` function of a runtime interface.
pub struct Wrapper {
	name: syn::Ident,
	item: TraitItemFn,
	/// `None` means the wrapper exists in every ABI epoch.
	abi_epoch: Option<u32>,
}

/// All functions of a runtime interface grouped by the function names.
pub struct RuntimeInterface {
	items: BTreeMap<syn::Ident, RuntimeInterfaceFunctionSet>,
	wrappers: Vec<Wrapper>,
}

impl RuntimeInterface {
	/// Returns an iterator over all runtime interface function
	/// [`latest_version_to_call`](RuntimeInterfaceFunctionSet::latest_version).
	pub fn latest_versions_to_call(
		&self,
	) -> impl Iterator<Item = (u32, &RuntimeInterfaceFunction)> {
		self.items.iter().filter_map(|(_, item)| item.latest_version_to_call())
	}

	/// Returns an iterator over the versions the bare functions must call when the gated ABI
	/// epochs are compiled out, for the functions where that version differs from
	/// [`Self::latest_versions_to_call`].
	pub fn legacy_versions_to_call(
		&self,
	) -> impl Iterator<Item = (u32, &RuntimeInterfaceFunction)> {
		self.items.iter().filter_map(|(_, item)| item.legacy_version_to_call())
	}

	pub fn all_versions(&self) -> impl Iterator<Item = (u32, &RuntimeInterfaceFunction)> {
		self.items
			.iter()
			.flat_map(|(_, item)| item.versions.iter())
			.map(|(v, i)| (*v, i))
	}

	pub fn wrappers(&self) -> impl Iterator<Item = (&syn::Ident, &TraitItemFn)> {
		self.wrappers.iter().map(|wrapper| (&wrapper.name, &wrapper.item))
	}

	/// Returns whether a wrapper with the given name exists (in the first ABI epoch, in the
	/// gated ABI epochs).
	///
	/// A wrapper takes over the module-level name it is defined with, so no bare function with
	/// the same name must be generated for the epochs the wrapper exists in.
	pub fn wrapper_shadow_modes(&self, name: &syn::Ident) -> (bool, bool) {
		self.wrappers.iter().filter(|wrapper| &wrapper.name == name).fold(
			(false, false),
			|(legacy, gated), wrapper| match wrapper.abi_epoch {
				None => (true, true),
				Some(epoch) if epoch > ABI_EPOCH_LEGACY => (legacy, true),
				Some(_) => (true, gated),
			},
		)
	}
}

/// Generates the include for the runtime-interface crate.
pub fn generate_runtime_interface_include() -> TokenStream {
	match crate_name("sp-runtime-interface") {
		Ok(FoundCrate::Itself) => quote!(),
		Ok(FoundCrate::Name(crate_name)) => {
			let crate_name = Ident::new(&crate_name, Span::call_site());
			quote!(
				#[doc(hidden)]
				extern crate #crate_name as proc_macro_runtime_interface;
			)
		},
		Err(e) => {
			let err = Error::new(Span::call_site(), e).to_compile_error();
			quote!( #err )
		},
	}
}

/// Generates the access to the `sp-runtime-interface` crate.
pub fn generate_crate_access() -> TokenStream {
	if env::var("CARGO_PKG_NAME").unwrap() == "sp-runtime-interface" {
		quote!(sp_runtime_interface)
	} else {
		quote!(proc_macro_runtime_interface)
	}
}

/// Create the exchangeable host function identifier for the given function name.
pub fn create_exchangeable_host_function_ident(name: &Ident) -> Ident {
	Ident::new(&format!("host_{}", name), Span::call_site())
}

/// Create the host function identifier for the given function name.
pub fn create_host_function_ident(name: &Ident, version: u32, trait_name: &Ident) -> Ident {
	Ident::new(
		&format!("ext_{}_{}_version_{}", trait_name.to_string().to_snake_case(), name, version),
		Span::call_site(),
	)
}

/// Create the host function identifier for the given function name.
pub fn create_function_ident_with_version(name: &Ident, version: u32) -> Ident {
	Ident::new(&format!("{}_version_{}", name, version), Span::call_site())
}

/// Returns the function arguments of the given `Signature`, minus any `self` arguments.
pub fn get_function_arguments(sig: &Signature) -> impl Iterator<Item = PatType> + '_ {
	sig.inputs
		.iter()
		.filter_map(|a| match a {
			FnArg::Receiver(_) => None,
			FnArg::Typed(pat_type) => Some(pat_type),
		})
		.enumerate()
		.map(|(i, arg)| {
			let mut res = arg.clone();
			if let Pat::Wild(wild) = &*arg.pat {
				let ident =
					Ident::new(&format!("__runtime_interface_generated_{}_", i), wild.span());

				res.pat = Box::new(parse_quote!( #ident ))
			}

			res
		})
}

/// Returns the function argument names of the given `Signature`, minus any `self`.
pub fn get_function_argument_names(sig: &Signature) -> impl Iterator<Item = Box<Pat>> + '_ {
	get_function_arguments(sig).map(|pt| pt.pat)
}

/// Returns the function argument types of the given `Signature`, minus any `Self` type.
pub fn get_function_argument_types(sig: &Signature) -> impl Iterator<Item = Box<Type>> + '_ {
	get_function_arguments(sig).map(|pt| pt.ty)
}

/// Returns the function argument names and types, minus any `self`.
pub fn get_function_argument_names_and_types(
	sig: &Signature,
) -> impl Iterator<Item = (Box<Pat>, Box<Type>)> + '_ {
	get_function_arguments(sig).map(|pt| (pt.pat, pt.ty))
}

/// Returns an iterator over all trait methods for the given trait definition.
fn get_trait_methods(trait_def: &ItemTrait) -> impl Iterator<Item = &TraitItemFn> {
	trait_def.items.iter().filter_map(|i| match i {
		TraitItem::Fn(ref method) => Some(method),
		_ => None,
	})
}

/// The version attribute that can be found above a runtime interface function.
///
/// Supports the following formats:
/// - `#[version(1)]`
/// - `#[version(1, register_only)]`
///
/// While this struct is only for parsing the inner parts inside the `()`.
struct VersionAttribute {
	version: u32,
	register_only: Option<attributes::register_only>,
}

impl VersionAttribute {
	/// Is this function version callable?
	fn is_callable(&self) -> bool {
		self.register_only.is_none()
	}
}

impl Default for VersionAttribute {
	fn default() -> Self {
		Self { version: 1, register_only: None }
	}
}

impl Parse for VersionAttribute {
	fn parse(input: syn::parse::ParseStream) -> Result<Self> {
		let version: LitInt = input.parse()?;
		let register_only = if input.peek(token::Comma) {
			let _ = input.parse::<token::Comma>();
			Some(input.parse()?)
		} else {
			if !input.is_empty() {
				return Err(Error::new(input.span(), "Unexpected token, expected `,`."));
			}

			None
		};

		Ok(Self { version: version.base10_parse()?, register_only })
	}
}

/// Return [`VersionAttribute`], if present.
fn get_item_version(item: &TraitItemFn) -> Result<Option<VersionAttribute>> {
	item.attrs
		.iter()
		.find(|attr| attr.path().is_ident("version"))
		.map(|attr| attr.parse_args())
		.transpose()
}

/// Returns all runtime interface members, with versions.
pub fn get_runtime_interface(trait_def: &ItemTrait) -> Result<RuntimeInterface> {
	let mut functions: BTreeMap<syn::Ident, RuntimeInterfaceFunctionSet> = BTreeMap::new();
	let mut wrappers: Vec<Wrapper> = Vec::new();

	for item in get_trait_methods(trait_def) {
		let name = item.sig.ident.clone();
		let is_wrapper = item.attrs.iter().any(|attr| attr.path().is_ident("wrapper"));
		if is_wrapper {
			let mut item = item.clone();
			let abi_epoch = extract_abi_epoch(&mut item)?;
			// For a wrapper, `#[abi_epoch(N)]` means "this wrapper exists only in epoch N", so
			// (unlike for function versions, which are always registered by the host) the
			// first epoch is gated as well.
			if abi_epoch == Some(ABI_EPOCH_LEGACY) {
				item.attrs.push(abi_epoch_negative_cfg());
			}
			wrappers.push(Wrapper { name, item, abi_epoch });
			continue;
		}

		let version = get_item_version(item)?.unwrap_or_default();

		if version.version < 1 {
			return Err(Error::new(item.span(), "Version needs to be at least `1`."));
		}

		match functions.entry(name.clone()) {
			Entry::Vacant(entry) => {
				entry.insert(RuntimeInterfaceFunctionSet::new(version, item)?);
			},
			Entry::Occupied(mut entry) => {
				entry.get_mut().add_version(version, item)?;
			},
		}
	}

	for function in functions.values() {
		let mut next_expected = 1;
		let mut callable_cfg: Option<(String, Span)> = None;
		let mut last_epoch = ABI_EPOCH_LEGACY;
		for (version, item) in function.versions.iter() {
			if next_expected != *version {
				return Err(Error::new(
					item.span(),
					format!(
						"Unexpected version attribute: missing version '{}' for this function",
						next_expected
					),
				));
			}
			next_expected += 1;

			// The bare function of a legacy (non-gated) build falls back to the latest
			// first-epoch version, so the versions belonging to gated epochs must form the
			// upper contiguous range of the version numbers.
			if item.abi_epoch() < last_epoch {
				return Err(Error::new(
					item.span(),
					"A newer version of a runtime interface function cannot belong to an \
					 older ABI epoch than its predecessor",
				));
			}
			last_epoch = item.abi_epoch();

			// Conditional compilation of function versions must keep the interface consistent:
			// the host must always register every version, and the version selected as the
			// bare function must not change depending on which versions are compiled in.
			// ABI epochs are exempt: they are the sanctioned way of gating versions, and the
			// bare function dispatch is epoch-aware.
			let cfg = item.declared_cfg_attrs.clone();
			if item.is_register_only() {
				if !cfg.is_empty() {
					return Err(Error::new(
						item.span(),
						"`register_only` versions cannot have `#[cfg]` attributes: \
						 the host must always register them",
					));
				}
			} else {
				match &callable_cfg {
					None => callable_cfg = Some((cfg, item.span())),
					Some((first_cfg, first_span)) => {
						if *first_cfg != cfg {
							let mut err = Error::new(
								item.span(),
								"All callable versions of a runtime interface function must \
								 have identical `#[cfg]` attributes; mark the superseded \
								 version as `register_only`, align the attributes, or use \
								 `#[abi_epoch]` to gate an ABI epoch",
							);
							err.combine(Error::new(
								*first_span,
								"Callable version with different `#[cfg]` attributes \
								 defined here",
							));
							return Err(err);
						}
					},
				}
			}
		}
	}

	Ok(RuntimeInterface { items: functions, wrappers })
}

pub fn host_inner_arg_ty(ty: &syn::Type) -> syn::Type {
	let crate_ = generate_crate_access();
	syn::parse2::<syn::Type>(quote! { <#ty as #crate_::RIType>::Inner })
		.expect("parsing doesn't fail")
}

pub fn pat_ty_to_host_inner(mut pat: syn::PatType) -> syn::PatType {
	pat.ty = Box::new(host_inner_arg_ty(&pat.ty));
	pat
}

pub fn host_inner_return_ty(ty: &syn::ReturnType) -> syn::ReturnType {
	let crate_ = generate_crate_access();
	match ty {
		syn::ReturnType::Default => syn::ReturnType::Default,
		syn::ReturnType::Type(ref arrow, ref ty) => {
			syn::parse2::<syn::ReturnType>(quote! { #arrow <#ty as #crate_::RIType>::Inner })
				.expect("parsing doesn't fail")
		},
	}
}

pub fn unpack_inner_types_in_signature(sig: &mut syn::Signature) {
	sig.output = crate::utils::host_inner_return_ty(&sig.output);
	for arg in sig.inputs.iter_mut() {
		match arg {
			syn::FnArg::Typed(ref mut pat_ty) => {
				*pat_ty = crate::utils::pat_ty_to_host_inner(pat_ty.clone());
			},
			syn::FnArg::Receiver(..) => {},
		}
	}
}
