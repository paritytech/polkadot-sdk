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

use crate::common::API_VERSION_ATTRIBUTE;
use inflector::Inflector;
use proc_macro2::{Span, TokenStream};
use proc_macro_crate::{crate_name, FoundCrate};
use quote::{format_ident, quote};
use syn::{
	parenthesized, parse_quote, punctuated::Punctuated, spanned::Spanned, token::And, Attribute,
	Error, Expr, ExprLit, FnArg, GenericArgument, Ident, ItemImpl, Lit, LitInt, LitStr, Meta,
	MetaNameValue, Pat, Path, PathArguments, Result, ReturnType, Signature, Token, Type, TypePath,
};

/// Generates the access to the `sc_client` crate.
pub fn generate_crate_access() -> TokenStream {
	match crate_name("sp-api") {
		Ok(FoundCrate::Itself) => quote!(sp_api::__private),
		Ok(FoundCrate::Name(renamed_name)) => {
			let renamed_name = Ident::new(&renamed_name, Span::call_site());
			quote!(#renamed_name::__private)
		},
		Err(e) => {
			if let Ok(FoundCrate::Name(name)) =
				crate_name(&"polkadot-sdk-frame").or_else(|_| crate_name(&"frame"))
			{
				let path = format!("{}::deps::sp_api::__private", name);
				let path = syn::parse_str::<syn::Path>(&path).expect("is a valid path; qed");
				quote!( #path )
			} else if let Ok(FoundCrate::Name(name)) = crate_name(&"polkadot-sdk") {
				let path = format!("{}::sp_api::__private", name);
				let path = syn::parse_str::<syn::Path>(&path).expect("is a valid path; qed");
				quote!( #path )
			} else {
				let err = Error::new(Span::call_site(), e).to_compile_error();
				quote!( #err )
			}
		},
	}
}

/// Generates the name of the module that contains the trait declaration for the runtime.
pub fn generate_runtime_mod_name_for_trait(trait_: &Ident) -> Ident {
	Ident::new(
		&format!("runtime_decl_for_{}", trait_.to_string().to_snake_case()),
		Span::call_site(),
	)
}

/// Get the type of a `syn::ReturnType`.
pub fn return_type_extract_type(rt: &ReturnType) -> Type {
	match rt {
		ReturnType::Default => parse_quote!(()),
		ReturnType::Type(_, ref ty) => *ty.clone(),
	}
}

/// Replace the `_` (wild card) parameter names in the given signature with unique identifiers.
pub fn replace_wild_card_parameter_names(input: &mut Signature) {
	let mut generated_pattern_counter = 0;
	input.inputs.iter_mut().for_each(|arg| {
		if let FnArg::Typed(arg) = arg {
			arg.pat =
				Box::new(sanitize_pattern((*arg.pat).clone(), &mut generated_pattern_counter));
		}
	});
}

/// Fold the given `Signature` to make it usable on the client side.
pub fn fold_fn_decl_for_client_side(
	input: &mut Signature,
	block_hash: &TokenStream,
	crate_: &TokenStream,
) {
	replace_wild_card_parameter_names(input);

	// Add `&self, at:& Block::Hash` as parameters to each function at the beginning.
	input.inputs.insert(0, parse_quote!( __runtime_api_at_param__: #block_hash ));
	input.inputs.insert(0, parse_quote!(&self));

	// Wrap the output in a `Result`
	input.output = {
		let ty = return_type_extract_type(&input.output);
		parse_quote!( -> std::result::Result<#ty, #crate_::ApiError> )
	};
}

/// Sanitize the given pattern.
///
/// - `_` patterns are changed to a variable based on `counter`.
/// - `mut something` removes the `mut`.
pub fn sanitize_pattern(pat: Pat, counter: &mut u32) -> Pat {
	match pat {
		Pat::Wild(_) => {
			let generated_name =
				Ident::new(&format!("__runtime_api_generated_name_{}__", counter), pat.span());
			*counter += 1;

			parse_quote!( #generated_name )
		},
		Pat::Ident(mut pat) => {
			pat.mutability = None;
			pat.into()
		},
		_ => pat,
	}
}

/// Allow `&self` in parameters of a method.
pub enum AllowSelfRefInParameters {
	/// Allows `&self` in parameters, but doesn't return it as part of the parameters.
	YesButIgnore,
	No,
}

/// Extracts the name, the type and `&` or ``(if it is a reference or not)
/// for each parameter in the given function signature.
pub fn extract_parameter_names_types_and_borrows(
	sig: &Signature,
	allow_self: AllowSelfRefInParameters,
) -> Result<Vec<(Pat, Type, Option<And>)>> {
	let mut result = Vec::new();
	let mut generated_pattern_counter = 0;
	for input in sig.inputs.iter() {
		match input {
			FnArg::Typed(arg) => {
				let (ty, borrow) = match &*arg.ty {
					Type::Reference(t) => ((*t.elem).clone(), Some(t.and_token)),
					t => (t.clone(), None),
				};

				let name = sanitize_pattern((*arg.pat).clone(), &mut generated_pattern_counter);
				result.push((name, ty, borrow));
			},
			FnArg::Receiver(_) if matches!(allow_self, AllowSelfRefInParameters::No) =>
				return Err(Error::new(input.span(), "`self` parameter not supported!")),
			FnArg::Receiver(recv) =>
				if recv.mutability.is_some() || recv.reference.is_none() {
					return Err(Error::new(recv.span(), "Only `&self` is supported!"));
				},
		}
	}

	Ok(result)
}

/// Prefix the given function with the trait name.
pub fn prefix_function_with_trait<F: ToString>(trait_: &Ident, function: &F) -> String {
	format!("{}_{}", trait_, function.to_string())
}

/// Extracts the block type from a trait path.
///
/// It is expected that the block type is the first type in the generic arguments.
pub fn extract_block_type_from_trait_path(trait_: &Path) -> Result<&TypePath> {
	let span = trait_.span();
	let generics = trait_
		.segments
		.last()
		.ok_or_else(|| Error::new(span, "Empty path not supported"))?;

	match &generics.arguments {
		PathArguments::AngleBracketed(ref args) => args
			.args
			.first()
			.and_then(|v| match v {
				GenericArgument::Type(Type::Path(ref block)) => Some(block),
				_ => None,
			})
			.ok_or_else(|| Error::new(args.span(), "Missing `Block` generic parameter.")),
		PathArguments::None => {
			let span = trait_.segments.last().as_ref().unwrap().span();
			Err(Error::new(span, "Missing `Block` generic parameter."))
		},
		PathArguments::Parenthesized(_) =>
			Err(Error::new(generics.arguments.span(), "Unexpected parentheses in path!")),
	}
}

/// Should a qualified trait path be required?
///
/// e.g. `path::Trait` is qualified and `Trait` is not.
pub enum RequireQualifiedTraitPath {
	Yes,
	No,
}

/// Extract the trait that is implemented by the given `ItemImpl`.
pub fn extract_impl_trait(impl_: &ItemImpl, require: RequireQualifiedTraitPath) -> Result<&Path> {
	impl_
		.trait_
		.as_ref()
		.map(|v| &v.1)
		.ok_or_else(|| Error::new(impl_.span(), "Only implementation of traits are supported!"))
		.and_then(|p| {
			if p.segments.len() > 1 || matches!(require, RequireQualifiedTraitPath::No) {
				Ok(p)
			} else {
				Err(Error::new(
					p.span(),
					"The implemented trait has to be referenced with a path, \
					e.g. `impl client::Core for Runtime`.",
				))
			}
		})
}

/// Parse the given attribute as `API_VERSION_ATTRIBUTE`.
pub fn parse_runtime_api_version(version: &Attribute) -> Result<u32> {
	let version = version.parse_args::<syn::LitInt>().map_err(|_| {
		Error::new(
			version.span(),
			&format!(
				"Unexpected `{api_version}` attribute. The supported format is `{api_version}(1)`",
				api_version = API_VERSION_ATTRIBUTE
			),
		)
	})?;

	version.base10_parse()
}

/// Each versioned trait is named 'ApiNameVN' where N is the specific version. E.g. ParachainHostV2
pub fn versioned_trait_name(trait_ident: &Ident, version: u32) -> Ident {
	format_ident!("{}V{}", trait_ident, version)
}

/// Extract the documentation from the provided attributes.
pub fn get_doc_literals(attrs: &[syn::Attribute]) -> Vec<syn::Lit> {
	use quote::ToTokens;
	attrs
		.iter()
		.filter_map(|attr| {
			let syn::Meta::NameValue(meta) = &attr.meta else { return None };
			let Ok(lit) = syn::parse2::<syn::Lit>(meta.value.to_token_stream()) else {
				unreachable!("non-lit doc attribute values do not exist");
			};
			meta.path.get_ident().filter(|ident| *ident == "doc").map(|_| lit)
		})
		.collect()
}

/// Filters all attributes except the cfg ones.
pub fn filter_cfg_attributes(attrs: &[syn::Attribute]) -> Vec<syn::Attribute> {
	attrs.iter().filter(|a| a.path().is_ident("cfg")).cloned().collect()
}

fn deprecation_msg_formatter(msg: &str) -> String {
	format!(
		r#"{msg}
		help: the following are the possible correct uses
|
|     #[deprecated = "reason"]
|
|     #[deprecated(/*opt*/ since = "version", /*opt*/ note = "reason")]
|
|     #[deprecated]
|"#
	)
}

fn parse_deprecated_meta(crate_: &TokenStream, attr: &syn::Attribute) -> Result<TokenStream> {
	match &attr.meta {
		Meta::List(meta_list) => {
			let parsed = meta_list
				.parse_args_with(Punctuated::<MetaNameValue, Token![,]>::parse_terminated)
				.map_err(|e| Error::new(attr.span(), e.to_string()))?;
			let (note, since) = parsed.iter().try_fold((None, None), |mut acc, item| {
				let value = match &item.value {
					Expr::Lit(ExprLit { lit: lit @ Lit::Str(_), .. }) => Ok(lit),
					_ => Err(Error::new(
						attr.span(),
						deprecation_msg_formatter(
							"Invalid deprecation attribute: expected string literal",
						),
					)),
				}?;
				if item.path.is_ident("note") {
					acc.0.replace(value);
				} else if item.path.is_ident("since") {
					acc.1.replace(value);
				}
				Ok::<(Option<&syn::Lit>, Option<&syn::Lit>), Error>(acc)
			})?;
			note.map_or_else(
				|| Err(Error::new(attr.span(), 						deprecation_msg_formatter(
					"Invalid deprecation attribute: missing `note`"))),
				|note| {
					let since = if let Some(str) = since {
						quote! { Some(#str) }
					} else {
						quote! { None }
					};
					let doc = quote! { #crate_::metadata_ir::ItemDeprecationInfoIR::Deprecated { note: #note, since: #since }};
					Ok(doc)
				},
			)
		},
		Meta::NameValue(MetaNameValue {
			value: Expr::Lit(ExprLit { lit: lit @ Lit::Str(_), .. }),
			..
		}) => {
			// #[deprecated = "lit"]
			let doc = quote! { #crate_::metadata_ir::ItemDeprecationInfoIR::Deprecated { note: #lit, since: None } };
			Ok(doc)
		},
		Meta::Path(_) => {
			// #[deprecated]
			Ok(quote! { #crate_::metadata_ir::ItemDeprecationInfoIR::DeprecatedWithoutNote })
		},
		_ => Err(Error::new(
			attr.span(),
			deprecation_msg_formatter("Invalid deprecation attribute: expected string literal"),
		)),
	}
}

/// collects deprecation attribute if its present.
pub fn get_deprecation(crate_: &TokenStream, attrs: &[syn::Attribute]) -> Result<TokenStream> {
	attrs
		.iter()
		.find(|a| a.path().is_ident("deprecated"))
		.map(|a| parse_deprecated_meta(&crate_, a))
		.unwrap_or_else(|| Ok(quote! {#crate_::metadata_ir::ItemDeprecationInfoIR::NotDeprecated}))
}

/// Represents an API version.
pub struct ApiVersion {
	/// Corresponds to `#[api_version(X)]` attribute.
	pub custom: Option<u32>,
	/// Corresponds to `#[cfg_attr(feature = "enable-staging-api", api_version(99))]`
	/// attribute. `String` is the feature name, `u32` the staging api version.
	pub feature_gated: Option<(String, u32)>,
}

/// Extracts the value of `API_VERSION_ATTRIBUTE` and handles errors.
/// Returns:
/// - Err if the version is malformed
/// - `ApiVersion` on success. If a version is set or not is determined by the fields of
///   `ApiVersion`
pub fn extract_api_version(attrs: &[Attribute], span: Span) -> Result<ApiVersion> {
	// First fetch all `API_VERSION_ATTRIBUTE` values (should be only one)
	let api_ver = attrs
		.iter()
		.filter(|a| a.path().is_ident(API_VERSION_ATTRIBUTE))
		.collect::<Vec<_>>();

	if api_ver.len() > 1 {
		return Err(Error::new(
			span,
			format!(
				"Found multiple #[{}] attributes for an API implementation. \
				Each runtime API can have only one version.",
				API_VERSION_ATTRIBUTE
			),
		));
	}

	// Parse the runtime version if there exists one.
	Ok(ApiVersion {
		custom: api_ver.first().map(|v| parse_runtime_api_version(v)).transpose()?,
		feature_gated: extract_cfg_api_version(attrs, span)?,
	})
}

/// Parse feature flagged api_version.
/// E.g. `#[cfg_attr(feature = "enable-staging-api", api_version(99))]`
fn extract_cfg_api_version(attrs: &[Attribute], span: Span) -> Result<Option<(String, u32)>> {
	let cfg_attrs = attrs.iter().filter(|a| a.path().is_ident("cfg_attr")).collect::<Vec<_>>();

	let mut cfg_api_version_attr = Vec::new();
	for cfg_attr in cfg_attrs {
		let mut feature_name = None;
		let mut api_version = None;
		cfg_attr.parse_nested_meta(|m| {
			if m.path.is_ident("feature") {
				let a = m.value()?;
				let b: LitStr = a.parse()?;
				feature_name = Some(b.value());
			} else if m.path.is_ident(API_VERSION_ATTRIBUTE) {
				let content;
				parenthesized!(content in m.input);
				let ver: LitInt = content.parse()?;
				api_version = Some(ver.base10_parse::<u32>()?);
			}
			Ok(())
		})?;

		// If there is a cfg attribute containing api_version - save if for processing
		if let (Some(feature_name), Some(api_version)) = (feature_name, api_version) {
			cfg_api_version_attr.push((feature_name, api_version, cfg_attr.span()));
		}
	}

	if cfg_api_version_attr.len() > 1 {
		let mut err = Error::new(span, format!("Found multiple feature gated api versions (cfg attribute with nested `{}` attribute). This is not supported.", API_VERSION_ATTRIBUTE));
		for (_, _, attr_span) in cfg_api_version_attr {
			err.combine(Error::new(attr_span, format!("`{}` found here", API_VERSION_ATTRIBUTE)));
		}

		return Err(err);
	}

	Ok(cfg_api_version_attr
		.into_iter()
		.next()
		.map(|(feature, name, _)| (feature, name)))
}

#[cfg(test)]
mod tests {
	use assert_matches::assert_matches;

	use super::*;

	#[test]
	fn check_get_doc_literals() {
		const FIRST: &'static str = "hello";
		const SECOND: &'static str = "WORLD";

		let doc: Attribute = parse_quote!(#[doc = #FIRST]);
		let doc_world: Attribute = parse_quote!(#[doc = #SECOND]);

		let attrs = vec![
			doc.clone(),
			parse_quote!(#[derive(Debug)]),
			parse_quote!(#[test]),
			parse_quote!(#[allow(non_camel_case_types)]),
			doc_world.clone(),
		];

		let docs = get_doc_literals(&attrs);
		assert_eq!(docs.len(), 2);
		assert_matches!(&docs[0], syn::Lit::Str(val) if val.value() == FIRST);
		assert_matches!(&docs[1], syn::Lit::Str(val) if val.value() == SECOND);
	}

	#[test]
	fn check_filter_cfg_attributes() {
		let cfg_std: Attribute = parse_quote!(#[cfg(feature = "std")]);
		let cfg_benchmarks: Attribute = parse_quote!(#[cfg(feature = "runtime-benchmarks")]);

		let attrs = vec![
			cfg_std.clone(),
			parse_quote!(#[derive(Debug)]),
			parse_quote!(#[test]),
			cfg_benchmarks.clone(),
			parse_quote!(#[allow(non_camel_case_types)]),
		];

		let filtered = filter_cfg_attributes(&attrs);
		assert_eq!(filtered.len(), 2);
		assert_eq!(cfg_std, filtered[0]);
		assert_eq!(cfg_benchmarks, filtered[1]);
	}

	#[test]
	fn check_deprecated_attr() {
		const FIRST: &'static str = "hello";
		const SECOND: &'static str = "WORLD";

		let simple: Attribute = parse_quote!(#[deprecated]);
		let simple_path: Attribute = parse_quote!(#[deprecated = #FIRST]);
		let meta_list: Attribute = parse_quote!(#[deprecated(note = #FIRST)]);
		let meta_list_with_since: Attribute =
			parse_quote!(#[deprecated(note = #FIRST, since = #SECOND)]);
		let extra_fields: Attribute =
			parse_quote!(#[deprecated(note = #FIRST, since = #SECOND, extra = "Test")]);
		assert_eq!(
			get_deprecation(&quote! { crate }, &[simple]).unwrap().to_string(),
			quote! { crate::metadata_ir::ItemDeprecationInfoIR::DeprecatedWithoutNote }.to_string()
		);
		assert_eq!(
			get_deprecation(&quote! { crate }, &[simple_path]).unwrap().to_string(),
			quote! { crate::metadata_ir::ItemDeprecationInfoIR::Deprecated { note: #FIRST, since: None } }.to_string()
		);
		assert_eq!(
			get_deprecation(&quote! { crate }, &[meta_list]).unwrap().to_string(),
			quote! { crate::metadata_ir::ItemDeprecationInfoIR::Deprecated { note: #FIRST, since: None } }.to_string()
		);
		assert_eq!(
			get_deprecation(&quote! { crate }, &[meta_list_with_since]).unwrap().to_string(),
			quote! { crate::metadata_ir::ItemDeprecationInfoIR::Deprecated { note: #FIRST, since: Some(#SECOND) }}.to_string()
		);
		assert_eq!(
			get_deprecation(&quote! { crate }, &[extra_fields]).unwrap().to_string(),
			quote! { crate::metadata_ir::ItemDeprecationInfoIR::Deprecated { note: #FIRST, since: Some(#SECOND) }}.to_string()
		);
	}
}
