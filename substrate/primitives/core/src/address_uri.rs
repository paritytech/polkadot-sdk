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

//! Little util for parsing an address URI. Replaces regular expressions.

/// A container for results of parsing the address uri string.
///
/// Intended to be equivalent of:
/// `Regex::new(r"^(?P<phrase>[a-zA-Z0-9 ]+)?(?P<path>(//?[^/]+)*)(///(?P<password>.*))?$")`
/// which also handles soft and hard derivation paths:
/// `Regex::new(r"/(/?[^/]+)")`
///
/// Example:
/// ```
/// 	use sp_core::crypto::AddressUri;
/// 	let manual_result = AddressUri::parse("hello world/s//h///pass");
/// 	assert_eq!(
/// 		manual_result.unwrap(),
/// 		AddressUri { phrase: Some("hello world"), paths: vec!["s", "/h"], pass: Some("pass") }
/// 	);
/// ```
#[derive(Debug, PartialEq)]
pub struct AddressUri<'a> {
	/// Phrase, hexadecimal string, or ss58-compatible string.
	pub phrase: Option<&'a str>,
	/// Key derivation paths, ordered as in input string,
	pub paths: Vec<&'a str>,
	/// Password.
	pub pass: Option<&'a str>,
}

/// Errors that are possible during parsing the address URI.
#[allow(missing_docs)]
#[derive(Debug, thiserror::Error, PartialEq, Eq, Clone, Copy)]
pub enum Error {
	#[error("Invalid character in phrase")]
	InvalidCharacterInPhrase,
	#[error("Invalid character in password")]
	InvalidCharacterInPass,
	#[error("Invalid character in hard path")]
	InvalidCharacterInHardPath,
	#[error("Invalid character in soft path")]
	InvalidCharacterInSoftPath,
}

fn extract_prefix<'a>(input: &mut &'a str, is_allowed: &dyn Fn(char) -> bool) -> Option<&'a str> {
	let output = input.trim_start_matches(is_allowed);
	let prefix_len = input.len() - output.len();
	let prefix = if prefix_len > 0 { Some(&input[..prefix_len]) } else { None };
	*input = output;
	prefix
}

impl<'a> AddressUri<'a> {
	/// Parses the given string.
	pub fn parse(mut input: &'a str) -> Result<Self, Error> {
		let phrase = extract_prefix(&mut input, &|ch: char| {
			ch.is_ascii_digit() || ch.is_ascii_alphabetic() || ch == ' '
		});

		let mut pass = None;
		let mut paths = Vec::new();
		while !input.is_empty() {
			input = if let Some(mut maybe_pass) = input.strip_prefix("///") {
				pass = match extract_prefix(&mut maybe_pass, &|ch: char| ch != '\n') {
					Some(pass) => Some(pass),
					None => Some(""),
				};
				maybe_pass
			} else if let Some(mut maybe_hard) = input.strip_prefix("//") {
				let mut path = extract_prefix(&mut maybe_hard, &|ch: char| ch != '/')
					.ok_or(Error::InvalidCharacterInHardPath)?;
				assert!(path.len() > 0);
				// hard path shall contain leading '/', so take it from input.
				path = &input[1..path.len() + 2];
				paths.push(path);
				maybe_hard
			} else if let Some(mut maybe_soft) = input.strip_prefix("/") {
				paths.push(
					extract_prefix(&mut maybe_soft, &|ch: char| ch != '/')
						.ok_or(Error::InvalidCharacterInSoftPath)?,
				);
				maybe_soft
			} else {
				return if pass.is_some() {
					Err(Error::InvalidCharacterInPass)
				} else {
					Err(Error::InvalidCharacterInPhrase)
				};
			}
		}

		Ok(Self { phrase, paths, pass })
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use regex::Regex;

	lazy_static::lazy_static! {
		static ref SECRET_PHRASE_REGEX: Regex = Regex::new(r"^(?P<phrase>[a-zA-Z0-9 ]+)?(?P<path>(//?[^/]+)*)(///(?P<password>.*))?$")
			.expect("constructed from known-good static value; qed");
	}

	fn check_with_regex(input: &str) {
		let regex_result = SECRET_PHRASE_REGEX.captures(input);
		let manual_result = AddressUri::parse(input);
		assert_eq!(regex_result.is_some(), manual_result.is_ok());
		if let (Some(regex_result), Ok(manual_result)) = (regex_result, manual_result) {
			assert_eq!(
				regex_result.name("phrase").map(|phrase| phrase.as_str()),
				manual_result.phrase
			);

			let manual_paths = manual_result
				.paths
				.iter()
				.map(|s| "/".to_string() + s)
				.collect::<Vec<_>>()
				.join("");

			assert_eq!(regex_result.name("path").unwrap().as_str().to_string(), manual_paths);
			assert_eq!(
				regex_result.name("password").map(|phrase| phrase.as_str()),
				manual_result.pass
			);
		}
	}

	fn check(input: &str, result: Result<AddressUri, Error>) {
		let manual_result = AddressUri::parse(input);
		assert_eq!(manual_result, result);
		check_with_regex(input);
	}

	#[test]
	fn test00() {
		check("///", Ok(AddressUri { phrase: None, pass: Some(""), paths: vec![] }));
	}

	#[test]
	fn test01() {
		check("////////", Ok(AddressUri { phrase: None, pass: Some("/////"), paths: vec![] }))
	}

	#[test]
	fn test02() {
		check(
			"sdasd///asda",
			Ok(AddressUri { phrase: Some("sdasd"), pass: Some("asda"), paths: vec![] }),
		);
	}

	#[test]
	fn test03() {
		check(
			"sdasd//asda",
			Ok(AddressUri { phrase: Some("sdasd"), pass: None, paths: vec!["/asda"] }),
		);
	}

	#[test]
	fn test04() {
		check("sdasd//a", Ok(AddressUri { phrase: Some("sdasd"), pass: None, paths: vec!["/a"] }));
	}

	#[test]
	fn test05() {
		check("sdasd//", Err(Error::InvalidCharacterInHardPath));
		//
	}

	#[test]
	fn test06() {
		check(
			"sdasd/xx//asda",
			Ok(AddressUri { phrase: Some("sdasd"), pass: None, paths: vec!["xx", "/asda"] }),
		);
	}

	#[test]
	fn test07() {
		check(
			"sdasd/xx//a/b//c///pass",
			Ok(AddressUri {
				phrase: Some("sdasd"),
				pass: Some("pass"),
				paths: vec!["xx", "/a", "b", "/c"],
			}),
		);
	}

	#[test]
	fn test08() {
		check(
			"sdasd/xx//a",
			Ok(AddressUri { phrase: Some("sdasd"), pass: None, paths: vec!["xx", "/a"] }),
		);
	}

	#[test]
	fn test09() {
		check("sdasd/xx//", Err(Error::InvalidCharacterInHardPath));
	}

	#[test]
	fn test10() {
		check(
			"sdasd/asda",
			Ok(AddressUri { phrase: Some("sdasd"), pass: None, paths: vec!["asda"] }),
		);
	}

	#[test]
	fn test11() {
		check(
			"sdasd/asda//x",
			Ok(AddressUri { phrase: Some("sdasd"), pass: None, paths: vec!["asda", "/x"] }),
		);
	}

	#[test]
	fn test12() {
		check("sdasd/a", Ok(AddressUri { phrase: Some("sdasd"), pass: None, paths: vec!["a"] }));
	}

	#[test]
	fn test13() {
		check("sdasd/", Err(Error::InvalidCharacterInSoftPath));
	}

	#[test]
	fn test14() {
		check("sdasd", Ok(AddressUri { phrase: Some("sdasd"), pass: None, paths: vec![] }));
	}

	#[test]
	fn test15() {
		check("sd.asd", Err(Error::InvalidCharacterInPhrase));
	}

	#[test]
	fn test16() {
		check("sd.asd/asd.a", Err(Error::InvalidCharacterInPhrase));
	}

	#[test]
	fn test17() {
		check("sd.asd//asd.a", Err(Error::InvalidCharacterInPhrase));
	}

	#[test]
	fn test18() {
		check(
			"sdasd/asd.a",
			Ok(AddressUri { phrase: Some("sdasd"), pass: None, paths: vec!["asd.a"] }),
		);
	}

	#[test]
	fn test19() {
		check(
			"sdasd//asd.a",
			Ok(AddressUri { phrase: Some("sdasd"), pass: None, paths: vec!["/asd.a"] }),
		);
	}

	#[test]
	fn test20() {
		check("///\n", Err(Error::InvalidCharacterInPass));
	}

	#[test]
	fn test21() {
		check("///a\n", Err(Error::InvalidCharacterInPass));
	}

	#[test]
	fn test22() {
		check("sd.asd///asd.a\n", Err(Error::InvalidCharacterInPhrase));
	}
}
