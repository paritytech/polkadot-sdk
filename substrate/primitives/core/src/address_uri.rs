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

#[cfg(all(not(feature = "std"), any(feature = "serde", feature = "full_crypto")))]
use sp_std::{
	alloc::string::{String, ToString},
	vec::Vec,
};

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
#[cfg_attr(feature = "std", derive(thiserror::Error))]
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Error {
	#[cfg_attr(feature = "std", error("Invalid character in phrase:\n{0}"))]
	InvalidCharacterInPhrase(InvalidCharacterInfo),
	#[cfg_attr(feature = "std", error("Invalid character in password:\n{0}"))]
	InvalidCharacterInPass(InvalidCharacterInfo),
	#[cfg_attr(feature = "std", error("Missing character in hard path:\n{0}"))]
	MissingCharacterInHardPath(InvalidCharacterInfo),
	#[cfg_attr(feature = "std", error("Missing character in soft path:\n{0}"))]
	MissingCharacterInSoftPath(InvalidCharacterInfo),
}

impl Error {
	/// Creates an instance of `Error::InvalidCharacterInPhrase` using given parameters.
	pub fn in_phrase(input: &str, pos: usize) -> Self {
		Self::InvalidCharacterInPhrase(InvalidCharacterInfo::new(input, pos))
	}
	/// Creates an instance of `Error::InvalidCharacterInPass` using given parameters.
	pub fn in_pass(input: &str, pos: usize) -> Self {
		Self::InvalidCharacterInPass(InvalidCharacterInfo::new(input, pos))
	}
	/// Creates an instance of `Error::MissingCharacterInHardPath` using given parameters.
	pub fn in_hard_path(input: &str, pos: usize) -> Self {
		Self::MissingCharacterInHardPath(InvalidCharacterInfo::new(input, pos))
	}
	/// Creates an instance of `Error::MissingCharacterInSoftPath` using given parameters.
	pub fn in_soft_path(input: &str, pos: usize) -> Self {
		Self::MissingCharacterInSoftPath(InvalidCharacterInfo::new(input, pos))
	}
}

/// Complementary error information.
///
/// Strucutre contains complementary information about parsing address URI string.
/// String contains a copy of an original URI string, 0-based integer indicates position of invalid
/// character.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct InvalidCharacterInfo(String, usize);

impl InvalidCharacterInfo {
	fn new(info: &str, pos: usize) -> Self {
		Self(info.to_string(), pos)
	}
}

impl sp_std::fmt::Display for InvalidCharacterInfo {
	fn fmt(&self, f: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
		let (s, pos) = escape_string(&self.0, self.1);
		write!(f, "{s}\n{i}^", i = sp_std::iter::repeat(" ").take(pos).collect::<String>())
	}
}

/// Escapes the control characters in given string, and recomputes the position if some characters
/// were actually escaped.
fn escape_string(input: &str, pos: usize) -> (String, usize) {
	let mut out = String::with_capacity(2 * input.len());
	let mut out_pos = 0;
	input
		.chars()
		.enumerate()
		.map(|(i, c)| {
			let esc = |c| (i, Some('\\'), c, 2);
			match c {
				'\t' => esc('t'),
				'\n' => esc('n'),
				'\r' => esc('r'),
				'\x07' => esc('a'),
				'\x08' => esc('b'),
				'\x0b' => esc('v'),
				'\x0c' => esc('f'),
				_ => (i, None, c, 1),
			}
		})
		.for_each(|(i, maybe_escape, c, increment)| {
			maybe_escape.map(|e| out.push(e));
			out.push(c);
			if i < pos {
				out_pos += increment;
			}
		});
	(out, out_pos)
}

fn extract_prefix<'a>(input: &mut &'a str, is_allowed: &dyn Fn(char) -> bool) -> Option<&'a str> {
	let output = input.trim_start_matches(is_allowed);
	let prefix_len = input.len() - output.len();
	let prefix = if prefix_len > 0 { Some(&input[..prefix_len]) } else { None };
	*input = output;
	prefix
}

fn strip_prefix(input: &mut &str, prefix: &str) -> bool {
	if let Some(stripped_input) = input.strip_prefix(prefix) {
		*input = stripped_input;
		true
	} else {
		false
	}
}

impl<'a> AddressUri<'a> {
	/// Parses the given string.
	pub fn parse(mut input: &'a str) -> Result<Self, Error> {
		let initial_input = input;
		let initial_input_len = input.len();
		let phrase = extract_prefix(&mut input, &|ch: char| {
			ch.is_ascii_digit() || ch.is_ascii_alphabetic() || ch == ' '
		});

		let mut pass = None;
		let mut paths = Vec::new();
		while !input.is_empty() {
			let unstripped_input = input;
			if strip_prefix(&mut input, "///") {
				pass = Some(extract_prefix(&mut input, &|ch: char| ch != '\n').unwrap_or(""));
			} else if strip_prefix(&mut input, "//") {
				let path = extract_prefix(&mut input, &|ch: char| ch != '/')
					.ok_or(Error::in_hard_path(initial_input, initial_input_len - input.len()))?;
				assert!(path.len() > 0);
				// hard path shall contain leading '/', so take it from unstripped input.
				paths.push(&unstripped_input[1..path.len() + 2]);
			} else if strip_prefix(&mut input, "/") {
				paths.push(
					extract_prefix(&mut input, &|ch: char| ch != '/').ok_or(
						Error::in_soft_path(initial_input, initial_input_len - input.len()),
					)?,
				);
			} else {
				return Err(if pass.is_some() {
					Error::in_pass(initial_input, initial_input_len - input.len())
				} else {
					Error::in_phrase(initial_input, initial_input_len - input.len())
				});
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
		let input = "sdasd//";
		check(input, Err(Error::in_hard_path(input, 7)));
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
		let input = "sdasd/xx//";
		check(input, Err(Error::in_hard_path(input, 10)));
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
		let input = "sdasd/";
		check(input, Err(Error::in_soft_path(input, 6)));
	}

	#[test]
	fn test14() {
		check("sdasd", Ok(AddressUri { phrase: Some("sdasd"), pass: None, paths: vec![] }));
	}

	#[test]
	fn test15() {
		let input = "sdasd.";
		check(input, Err(Error::in_phrase(input, 5)));
	}

	#[test]
	fn test16() {
		let input = "sd.asd/asd.a";
		check(input, Err(Error::in_phrase(input, 2)));
	}

	#[test]
	fn test17() {
		let input = "sd.asd//asd.a";
		check(input, Err(Error::in_phrase(input, 2)));
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
		let input = "///\n";
		check(input, Err(Error::in_pass(input, 3)));
	}

	#[test]
	fn test21() {
		let input = "///a\n";
		check(input, Err(Error::in_pass(input, 4)));
	}

	#[test]
	fn test22() {
		let input = "sd asd///asd.a\n";
		check(input, Err(Error::in_pass(input, 14)));
	}

	#[test]
	fn test_invalid_char_info_1() {
		let expected = "01234\n^";
		let f = format!("{}", InvalidCharacterInfo::new("01234", 0));
		assert_eq!(expected, f);
	}

	#[test]
	fn test_invalid_char_info_2() {
		let expected = "01\n ^";
		let f = format!("{}", InvalidCharacterInfo::new("01", 1));
		assert_eq!(expected, f);
	}

	#[test]
	fn test_invalid_char_info_3() {
		let expected = "01234\n  ^";
		let f = format!("{}", InvalidCharacterInfo::new("01234", 2));
		assert_eq!(expected, f);
	}

	#[test]
	fn test_invalid_char_info_4() {
		let expected = "012\\n456\n   ^";
		let f = format!("{}", InvalidCharacterInfo::new("012\n456", 3));
		assert_eq!(expected, f);
	}

	#[test]
	fn test_invalid_char_info_5() {
		let expected = "012\\n456\n      ^";
		let f = format!("{}", InvalidCharacterInfo::new("012\n456", 5));
		assert_eq!(expected, f);
	}

	#[test]
	fn test_invalid_char_info_6() {
		let expected = "012\\f456\\t89\n           ^";
		let f = format!("{}", InvalidCharacterInfo::new("012\x0c456\t89", 9));
		assert_eq!(expected, f);
	}
}
