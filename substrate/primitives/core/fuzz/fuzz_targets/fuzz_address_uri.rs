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

#![no_main]

extern crate libfuzzer_sys;
extern crate regex;
extern crate sp_core;

use libfuzzer_sys::fuzz_target;
use regex::Regex;
use sp_core::crypto::AddressUri;

lazy_static::lazy_static! {
	static ref SECRET_PHRASE_REGEX: Regex = Regex::new(r"^(?P<phrase>[a-zA-Z0-9 ]+)?(?P<path>(//?[^/]+)*)(///(?P<password>.*))?$")
		.expect("constructed from known-good static value; qed");
}

fuzz_target!(|input: &str| {
	let regex_result = SECRET_PHRASE_REGEX.captures(input);
	let manual_result = AddressUri::parse(input);
	assert_eq!(regex_result.is_some(), manual_result.is_ok());
	if manual_result.is_err() {
		let _ = format!("{}", manual_result.as_ref().err().unwrap());
	}
	if let (Some(regex_result), Ok(manual_result)) = (regex_result, manual_result) {
		assert_eq!(regex_result.name("phrase").map(|p| p.as_str()), manual_result.phrase);

		let manual_paths = manual_result
			.paths
			.iter()
			.map(|s| "/".to_string() + s)
			.collect::<Vec<_>>()
			.join("");

		assert_eq!(regex_result.name("path").unwrap().as_str().to_string(), manual_paths);
		assert_eq!(regex_result.name("password").map(|pass| pass.as_str()), manual_result.pass);
	}
});
