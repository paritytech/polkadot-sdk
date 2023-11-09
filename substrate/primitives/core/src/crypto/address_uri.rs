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

#[derive(Debug, PartialEq)]
/// A container for results of parsing the address uri string.
pub struct AddressUri<'a> {
	pub ss58: Option<&'a str>,
	pub paths: Vec<&'a str>,
	pub pass: Option<&'a str>,
}

impl<'a> AddressUri<'a> {
	fn extract_prefix(input: &mut &'a str, is_allowed: &dyn Fn(char) -> bool) -> Option<&'a str> {
		let output = input.trim_start_matches(is_allowed);
		let prefix_len = input.len() - output.len();
		let prefix = if prefix_len > 0 { Some(&input[..prefix_len]) } else { None };
		*input = output;
		prefix
	}

	/// Parses the given string.
	///
	/// Intended to be equivalent of:
	/// Regex::new(r"^(?P<phrase>[a-zA-Z0-9 ]+)?(?P<path>(//?[^/]+)*)(///(?P<password>.*))?$")
	pub fn parse(mut input: &'a str) -> Option<Self> {
		let ss58 = Self::extract_prefix(&mut input, &|ch: char| {
			ch.is_ascii_digit() || ch.is_ascii_alphabetic() || ch == ' '
		});

		let mut pass = None;
		let mut paths = Vec::new();
		while !input.is_empty() {
			input = if let Some(mut maybe_pass) = input.strip_prefix("///") {
				pass = match Self::extract_prefix(&mut maybe_pass, &|ch: char| ch != '\n') {
					Some(pass) => Some(pass),
					None => Some(""),
				};
				maybe_pass
			} else if let Some(mut maybe_hard) = input.strip_prefix("//") {
				let Some(mut path) = Self::extract_prefix(&mut maybe_hard, &|ch: char| ch != '/')
				else {
					return None;
				};
				assert!(path.len() > 0);
				// hard path shall contain leading '/', so take it from input.
				path = &input[1..path.len() + 2];
				paths.push(path);
				maybe_hard
			} else if let Some(mut maybe_soft) = input.strip_prefix("/") {
				let Some(path) = Self::extract_prefix(&mut maybe_soft, &|ch: char| ch != '/')
				else {
					return None;
				};
				paths.push(path);
				maybe_soft
			} else {
				return None;
			}
		}

		Some(Self { ss58, paths, pass })
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use regex::Regex;

	lazy_static::lazy_static! {
		static ref SS58_REGEX: Regex = Regex::new(r"^(?P<ss58>[a-zA-Z0-9 ]+)?(?P<path>(//?[^/]+)*)$")
			.expect("constructed from known-good static value; qed");
		static ref SECRET_PHRASE_REGEX: Regex = Regex::new(r"^(?P<phrase>[a-zA-Z0-9 ]+)?(?P<path>(//?[^/]+)*)(///(?P<password>.*))?$")
			.expect("constructed from known-good static value; qed");
		static ref JUNCTION_REGEX: Regex = Regex::new(r"/(/?[^/]+)")
			.expect("constructed from known-good static value; qed");
	}

	fn check_with_regex(input: &str) {
		let regex_result = SECRET_PHRASE_REGEX.captures(input);
		let manual_result = AddressUri::parse(input);
		assert_eq!(regex_result.is_some(), manual_result.is_some());
		if let (Some(regex_result), Some(manual_result)) = (regex_result, manual_result) {
			assert_eq!(regex_result.name("phrase").map(|ss58| ss58.as_str()), manual_result.ss58);

			let manual_paths = manual_result
				.paths
				.iter()
				.map(|s| {
					let mut s = String::from(*s);
					s = "/".to_string() + &s;
					s
				})
				.collect::<Vec<_>>()
				.join("");

			assert_eq!(regex_result.name("path").unwrap().as_str().to_string(), manual_paths);
			assert_eq!(regex_result.name("password").map(|ss58| ss58.as_str()), manual_result.pass);
		}
	}

	fn check(input: &str, result: Option<AddressUri>) {
		let manual_result = AddressUri::parse(input);
		assert_eq!(manual_result, result);
		check_with_regex(input);
	}

	#[test]
	fn test00() {
		check("///", Some(AddressUri { ss58: None, pass: Some(""), paths: vec![] }));
	}

	#[test]
	fn test01() {
		check("////////", Some(AddressUri { ss58: None, pass: Some("/////"), paths: vec![] }))
	}

	#[test]
	fn test02() {
		check(
			"sdasd///asda",
			Some(AddressUri { ss58: Some("sdasd"), pass: Some("asda"), paths: vec![] }),
		);
		//
	}

	#[test]
	fn test03() {
		check(
			"sdasd//asda",
			Some(AddressUri { ss58: Some("sdasd"), pass: None, paths: vec!["/asda"] }),
		);
	}

	#[test]
	fn test04() {
		check("sdasd//a", Some(AddressUri { ss58: Some("sdasd"), pass: None, paths: vec!["/a"] }));
	}

	#[test]
	fn test05() {
		check("sdasd//", None);
		//
	}

	#[test]
	fn test06() {
		check(
			"sdasd/xx//asda",
			Some(AddressUri { ss58: Some("sdasd"), pass: None, paths: vec!["xx", "/asda"] }),
		);
	}

	#[test]
	fn test07() {
		check(
			"sdasd/xx//a/b//c///pass",
			Some(AddressUri {
				ss58: Some("sdasd"),
				pass: Some("pass"),
				paths: vec!["xx", "/a", "b", "/c"],
			}),
		);
	}

	#[test]
	fn test08() {
		check(
			"sdasd/xx//a",
			Some(AddressUri { ss58: Some("sdasd"), pass: None, paths: vec!["xx", "/a"] }),
		);
	}

	#[test]
	fn test09() {
		check("sdasd/xx//", None);
	}

	#[test]
	fn test10() {
		check(
			"sdasd/asda",
			Some(AddressUri { ss58: Some("sdasd"), pass: None, paths: vec!["asda"] }),
		);
	}

	#[test]
	fn test11() {
		check(
			"sdasd/asda//x",
			Some(AddressUri { ss58: Some("sdasd"), pass: None, paths: vec!["asda", "/x"] }),
		);
	}

	#[test]
	fn test12() {
		check("sdasd/a", Some(AddressUri { ss58: Some("sdasd"), pass: None, paths: vec!["a"] }));
	}

	#[test]
	fn test13() {
		check("sdasd/", None);
	}

	#[test]
	fn test14() {
		check("sdasd", Some(AddressUri { ss58: Some("sdasd"), pass: None, paths: vec![] }));
	}

	#[test]
	fn test15() {
		check("sd.asd", None);
	}

	#[test]
	fn test16() {
		check("sd.asd/asd.a", None);
	}

	#[test]
	fn test17() {
		check("sd.asd//asd.a", None);
	}

	#[test]
	fn test18() {
		check(
			"sdasd/asd.a",
			Some(AddressUri { ss58: Some("sdasd"), pass: None, paths: vec!["asd.a"] }),
		);
	}

	#[test]
	fn test19() {
		check(
			"sdasd//asd.a",
			Some(AddressUri { ss58: Some("sdasd"), pass: None, paths: vec!["/asd.a"] }),
		);
	}

	#[test]
	fn test20() {
		check("///\n", None);
	}

	#[test]
	fn test21() {
		check("///a\n", None);
	}
}
