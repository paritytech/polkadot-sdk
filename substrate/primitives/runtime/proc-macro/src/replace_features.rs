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

//! Provides functions to deal with feature replacement in cfg-style macro.

use syn::punctuated::Punctuated;

mod kw {
	syn::custom_keyword!(all);
	syn::custom_keyword!(any);
	syn::custom_keyword!(not);
}

/// Description of a runtime feature e.g. `sp-runtime/try-runtime` is enabled.
pub struct RuntimeFeature {
	pub name: String,
	pub is_enabled: bool,
}

/// Syntax for `cfg`-style macro. E.g. `any(feature = "foo", bar)`.
///
/// This should be a full implementation of the `cfg` macro:
/// https://doc.rust-lang.org/reference/conditional-compilation.html
///
///    Syntax
///    ConfigurationPredicate :
///          ConfigurationOption
///       | ConfigurationAll
///       | ConfigurationAny
///       | ConfigurationNot
///
///    ConfigurationOption :
///       IDENTIFIER (= (STRING_LITERAL | RAW_STRING_LITERAL))?
///
///    ConfigurationAll
///       all ( ConfigurationPredicateList? )
///
///    ConfigurationAny
///       any ( ConfigurationPredicateList? )
///
///    ConfigurationNot
///       not ( ConfigurationPredicate )
///
///    ConfigurationPredicateList
///       ConfigurationPredicate (, ConfigurationPredicate)* ,?
///
pub enum ConfigurationPredicate {
	Option(ConfigurationOption),
	All(ConfigurationList),
	Any(ConfigurationList),
	Not(ConfigurationNot),
}

/// E.g. `feature = "foo"`.
pub struct ConfigurationOption {
	ident: syn::Ident,
	value: Option<(syn::Token![=], syn::LitStr)>,
}

/// E.g. `all(feature = "foo", bar)` or `any(feature = "foo", bar)`.
pub struct ConfigurationList {
	ident: syn::Ident,
	predicates: Punctuated<ConfigurationPredicate, syn::Token![,]>,
}

/// E.g. `not(feature = "foo")`.
pub struct ConfigurationNot {
	ident: syn::Ident,
	predicate: Box<ConfigurationPredicate>,
}

impl syn::parse::Parse for ConfigurationPredicate {
	fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
		if input.peek(kw::all) && input.peek2(syn::token::Paren) {
			Ok(Self::All(input.parse()?))
		} else if input.peek(kw::any) && input.peek2(syn::token::Paren) {
			Ok(Self::Any(input.parse()?))
		} else if input.peek(kw::not) && input.peek2(syn::token::Paren) {
			Ok(Self::Not(input.parse()?))
		} else if !input.peek(kw::not) &&!input.peek(kw::any) &&!input.peek(kw::all) && input.peek(syn::Ident) {
			Ok(Self::Option(input.parse()?))
		} else {
			Err(input.error(
				"Expected `all(..)`, `any(..)`, `not(..)`, `some_feature`, or `some_feature = ..`",
			))
		}
	}
}

impl quote::ToTokens for ConfigurationPredicate {
	fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
		match self {
			ConfigurationPredicate::Option(option) => option.to_tokens(tokens),
			ConfigurationPredicate::All(all) => all.to_tokens(tokens),
			ConfigurationPredicate::Any(any) => any.to_tokens(tokens),
			ConfigurationPredicate::Not(not) => not.to_tokens(tokens),
		}
	}
}

impl syn::parse::Parse for ConfigurationOption {
	fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
		let ident: syn::Ident = input.parse()?;
		let value = if input.peek(syn::Token![=]) {
			let eq: syn::Token![=] = input.parse()?;
			let value: syn::LitStr = input.parse()?;
			Some((eq, value))
		} else {
			None
		};

		Ok(ConfigurationOption { ident, value })
	}
}

impl quote::ToTokens for ConfigurationOption {
	fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
		let ident = &self.ident;
		let value = self.value.as_ref().map(|(eq, v)| quote::quote!(#eq #v));
		tokens.extend(quote::quote!(#ident #value));
	}
}

impl syn::parse::Parse for ConfigurationList {
	fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
		let ident = input.parse()?;
		let content;
		syn::parenthesized!(content in input);
		let predicates =
			Punctuated::<ConfigurationPredicate, syn::Token![,]>::parse_terminated(&content)?;

		if predicates.is_empty() {
			return Err(content.error("Expected at least one predicate"));
		}

		Ok(ConfigurationList { ident, predicates })
	}
}

impl quote::ToTokens for ConfigurationList {
	fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
		let ident = &self.ident;
		let predicates = &self.predicates;
		tokens.extend(quote::quote!(#ident(#predicates)));
	}
}

impl syn::parse::Parse for ConfigurationNot {
	fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
		let ident = input.parse()?;
		let content;
		syn::parenthesized!(content in input);
		let predicate: ConfigurationPredicate = content.parse()?;
		Ok(ConfigurationNot { ident, predicate: Box::new(predicate) })
	}
}

impl quote::ToTokens for ConfigurationNot {
	fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
		let ident = &self.ident;
		let predicate = &self.predicate;
		tokens.extend(quote::quote!(#ident(#predicate)));
	}
}

impl ConfigurationPredicate {
	pub fn replace_features(&mut self, features: &[RuntimeFeature]) {
		match self {
			ConfigurationPredicate::All(ref mut list)
			| ConfigurationPredicate::Any(ref mut list) => {
				for predicate in &mut list.predicates {
					predicate.replace_features(features);
				}
			},
			ConfigurationPredicate::Not(ref mut not) => not.predicate.replace_features(features),
			ConfigurationPredicate::Option(ref mut option) => {
				if let Some((_, lit)) = &mut option.value {
					if option.ident.to_string() == "feature" {
						for feature in features {
							if lit.value() == feature.name {
								let false_predicate =
									ConfigurationPredicate::All(ConfigurationList {
										ident: syn::Ident::new("all", lit.span()),
										predicates: FromIterator::from_iter([
											ConfigurationPredicate::Option(ConfigurationOption {
												ident: syn::Ident::new("target_endian", lit.span()),
												value: Some((
													Default::default(),
													syn::LitStr::new("little", lit.span()),
												)),
											}),
											ConfigurationPredicate::Option(ConfigurationOption {
												ident: syn::Ident::new("target_endian", lit.span()),
												value: Some((
													Default::default(),
													syn::LitStr::new("big", lit.span()),
												)),
											}),
										]),
									});

								if feature.is_enabled {
									*self = ConfigurationPredicate::Not(ConfigurationNot {
										ident: syn::Ident::new("not", lit.span()),
										predicate: Box::new(false_predicate),
									});
								} else {
									*self = false_predicate;
								}

								return;
							}
						}
					}
				}
			},
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use quote::ToTokens;

	#[test]
	fn test_replace() {
		let input = quote::quote! {
			any(feature = "sp-runtime/try-runtime", not(feature = "sp-runtime/runtime-benchmarks"))
		};
		let features = [
			RuntimeFeature { name: "sp-runtime/try-runtime".into(), is_enabled: true },
			RuntimeFeature { name: "sp-runtime/runtime-benchmarks".into(), is_enabled: false },
		];
		let expected = quote::quote! {
			any(
				not(all(target_endian = "little", target_endian = "big")),
				not(
					all(target_endian = "little", target_endian = "big")
				)
			)
		};

		let mut input: ConfigurationPredicate = syn::parse2(input).unwrap();
		input.replace_features(&features[..]);

		assert_eq!(input.to_token_stream().to_string(), expected.to_string());
	}

	#[test]
	fn test_replace2() {
		let input = quote::quote! {
			any(foo, not(feature = "sp-runtime/runtime-benchmarks"), bar)
		};
		let features = [
			RuntimeFeature { name: "sp-runtime/try-runtime".into(), is_enabled: true },
			RuntimeFeature { name: "sp-runtime/runtime-benchmarks".into(), is_enabled: false },
		];
		let expected = quote::quote! {
			any(
				foo,
				not(
					all(target_endian = "little", target_endian = "big")
				),
				bar
			)
		};

		let mut input: ConfigurationPredicate = syn::parse2(input).unwrap();
		input.replace_features(&features[..]);

		assert_eq!(input.to_token_stream().to_string(), expected.to_string());
	}
}
