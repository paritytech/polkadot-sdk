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

use codec::Encode as ScaleEncode;
use jam_types::{
	Authorizer, BoundedVec, MaxWorkItems, RefineContext, ServiceId, WorkItem, WorkPackage,
};
use primitive_types::H256;

use crate::{signing::SigningPayload, AuthToken};

/// A work package awaiting its authorization token.
///
/// Nodes can compute the signing hash before attaching the completed token.
#[derive(Clone, Debug)]
pub struct WorkPackageBody {
	/// Service hosting the authorizer code.
	pub auth_code_host: ServiceId,
	/// Authorizer code hash and configuration.
	pub authorizer: Authorizer,
	/// Refinement context.
	pub context: RefineContext,
	/// Work items in execution order.
	pub items: BoundedVec<WorkItem, MaxWorkItems>,
}

impl WorkPackageBody {
	/// Hash to be signed by the collator when constructing an AURA token.
	pub fn signing_hash(&self) -> H256 {
		SigningPayload::from(self).hash()
	}

	/// Attach an existing AURA token and return the complete work package.
	///
	/// SCALE-encodes the token without creating a signature or verifying authorization.
	pub fn with_authorization(self, token: &AuthToken) -> WorkPackage {
		WorkPackage {
			authorization: ScaleEncode::encode(token).into(),
			auth_code_host: self.auth_code_host,
			authorizer: self.authorizer,
			context: self.context,
			items: self.items,
		}
	}
}
