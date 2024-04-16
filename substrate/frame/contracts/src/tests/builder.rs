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

use super::{AccountId32, Test, ALICE};
use crate::{test_utils::builder::*, tests::RuntimeOrigin, AccountIdLookupOf, Code, CodeHash};

/// Create a [`BareInstantiateBuilder`] with default values.
pub fn bare_instantiate(code: Code<CodeHash<Test>>) -> BareInstantiateBuilder<Test> {
	BareInstantiateBuilder::<Test>::bare_instantiate(ALICE, code)
}

/// Create a [`BareCallBuilder`] with default values.
pub fn bare_call(dest: AccountId32) -> BareCallBuilder<Test> {
	BareCallBuilder::<Test>::bare_call(ALICE, dest)
}

/// Create an [`InstantiateWithCodeBuilder`] with default values.
pub fn instantiate_with_code(code: Vec<u8>) -> InstantiateWithCodeBuilder<Test> {
	InstantiateWithCodeBuilder::<Test>::instantiate_with_code(RuntimeOrigin::signed(ALICE), code)
}

/// Create an [`InstantiateBuilder`] with default values.
pub fn instantiate(code_hash: CodeHash<Test>) -> InstantiateBuilder<Test> {
	InstantiateBuilder::<Test>::instantiate(RuntimeOrigin::signed(ALICE), code_hash)
}

/// Create a [`CallBuilder`] with default values.
pub fn call(dest: AccountIdLookupOf<Test>) -> CallBuilder<Test> {
	CallBuilder::<Test>::call(RuntimeOrigin::signed(ALICE), dest)
}
