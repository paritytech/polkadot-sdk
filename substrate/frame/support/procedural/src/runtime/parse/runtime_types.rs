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

use syn::{
	parse::{Parse, ParseStream},
	Result,
};

mod keyword {
	use syn::custom_keyword;

	custom_keyword!(RuntimeCall);
	custom_keyword!(RuntimeEvent);
	custom_keyword!(RuntimeError);
	custom_keyword!(RuntimeOrigin);
	custom_keyword!(RuntimeFreezeReason);
	custom_keyword!(RuntimeHoldReason);
	custom_keyword!(RuntimeSlashReason);
	custom_keyword!(RuntimeLockId);
	custom_keyword!(RuntimeTask);
}

#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeType {
	RuntimeCall(keyword::RuntimeCall),
	RuntimeEvent(keyword::RuntimeEvent),
	RuntimeError(keyword::RuntimeError),
	RuntimeOrigin(keyword::RuntimeOrigin),
	RuntimeFreezeReason(keyword::RuntimeFreezeReason),
	RuntimeHoldReason(keyword::RuntimeHoldReason),
	RuntimeSlashReason(keyword::RuntimeSlashReason),
	RuntimeLockId(keyword::RuntimeLockId),
	RuntimeTask(keyword::RuntimeTask),
}

impl Parse for RuntimeType {
	fn parse(input: ParseStream) -> Result<Self> {
		let lookahead = input.lookahead1();

		if lookahead.peek(keyword::RuntimeCall) {
			Ok(Self::RuntimeCall(input.parse()?))
		} else if lookahead.peek(keyword::RuntimeEvent) {
			Ok(Self::RuntimeEvent(input.parse()?))
		} else if lookahead.peek(keyword::RuntimeError) {
			Ok(Self::RuntimeError(input.parse()?))
		} else if lookahead.peek(keyword::RuntimeOrigin) {
			Ok(Self::RuntimeOrigin(input.parse()?))
		} else if lookahead.peek(keyword::RuntimeFreezeReason) {
			Ok(Self::RuntimeFreezeReason(input.parse()?))
		} else if lookahead.peek(keyword::RuntimeHoldReason) {
			Ok(Self::RuntimeHoldReason(input.parse()?))
		} else if lookahead.peek(keyword::RuntimeSlashReason) {
			Ok(Self::RuntimeSlashReason(input.parse()?))
		} else if lookahead.peek(keyword::RuntimeLockId) {
			Ok(Self::RuntimeLockId(input.parse()?))
		} else if lookahead.peek(keyword::RuntimeTask) {
			Ok(Self::RuntimeTask(input.parse()?))
		} else {
			Err(lookahead.error())
		}
	}
}
