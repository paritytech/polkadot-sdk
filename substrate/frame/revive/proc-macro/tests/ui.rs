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

#[test]
fn define_versioned_type_accepts_valid_macro_input() {
	// Arrange
	let cases = trybuild::TestCases::new();

	// Act
	let path = "tests/ui/define_versioned_type/pass/*.rs";

	// Assert
	cases.pass(path);
}

#[test]
fn define_versioned_type_rejects_invalid_macro_input() {
	// Arrange
	let cases = trybuild::TestCases::new();

	// Act
	let path = "tests/ui/define_versioned_type/fail/*.rs";

	// Assert
	cases.compile_fail(path);
}

#[test]
fn define_versioned_interface_rejects_invalid_macro_input() {
	// Arrange
	let cases = trybuild::TestCases::new();

	// Act
	let path = "tests/ui/define_versioned_interface/fail/*.rs";

	// Assert
	cases.compile_fail(path);
}
