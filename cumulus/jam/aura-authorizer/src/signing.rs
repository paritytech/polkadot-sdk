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

use alloc::vec::Vec;
use jam_types::{
	Authorizer, BoundedVec, Encode as JamEncode, MaxWorkItems, RefineContext, ServiceId, WorkItem,
	WorkPackage,
};
use primitive_types::H256;
use sp_crypto_hashing::blake2_256;

use crate::WorkPackageBody;

/// Domain separator for the token-free work-package hash signed by AURA collators.
pub const WORK_PACKAGE_SIGN_CTX: &[u8] = b"jam:parachain-service:aura:work-package:v1";

/// Hash of a work-package that can be signed by AURA collators.
///
/// JAM-encodes the code host, authorizer code hash, refinement context, authorizer config,
/// and work items after [`WORK_PACKAGE_SIGN_CTX`], then hashes them with Blake2b-256.
/// The authorization token is excluded because it contains the signature.
pub fn signable_work_package_hash(package: &WorkPackage) -> H256 {
	SigningPayload::from(package).hash()
}

/// Borrowed view of the work-package fields covered by the collator signature.
pub(crate) struct SigningPayload<'a> {
	auth_code_host: ServiceId,
	authorizer: &'a Authorizer,
	context: &'a RefineContext,
	items: &'a BoundedVec<WorkItem, MaxWorkItems>,
}

impl<'a> From<&'a WorkPackage> for SigningPayload<'a> {
	fn from(package: &'a WorkPackage) -> Self {
		// Keep this exhaustive so new fields require an explicit signing decision.
		let WorkPackage { authorization: _, auth_code_host, authorizer, context, items } = package;
		Self { auth_code_host: *auth_code_host, authorizer, context, items }
	}
}

impl<'a> From<&'a WorkPackageBody> for SigningPayload<'a> {
	fn from(package: &'a WorkPackageBody) -> Self {
		let WorkPackageBody { auth_code_host, authorizer, context, items } = package;
		Self { auth_code_host: *auth_code_host, authorizer, context, items }
	}
}

impl SigningPayload<'_> {
	pub(crate) fn hash(&self) -> H256 {
		let payload = JamEncode::encode(&(
			&self.auth_code_host,
			&self.authorizer.code_hash,
			self.context,
			&self.authorizer.config,
			self.items,
		));
		hash_signing_payload(&payload)
	}
}

/// Hash the JAM-encoded tuple of code host, authorizer code hash, refinement context,
/// authorizer config, and work items, with the AURA domain separator.
///
/// Adapters for other JAM versions must preserve their version's complete context encoding
/// and exclude the authorization token, just as [`signable_work_package_hash`] does.
pub fn hash_signing_payload(payload: &[u8]) -> H256 {
	let mut signable = Vec::with_capacity(WORK_PACKAGE_SIGN_CTX.len() + payload.len());
	signable.extend_from_slice(WORK_PACKAGE_SIGN_CTX);
	signable.extend_from_slice(payload);

	H256::from(blake2_256(&signable))
}

#[cfg(test)]
mod tests {
	use super::*;
	use alloc::vec;

	#[test]
	fn hash_signing_payload_vectors_works() {
		// Generated independently with Python hashlib.blake2b, digest_size=32.
		for (payload, expected) in [
			(
				&b""[..],
				[
					92, 48, 36, 169, 96, 170, 208, 175, 233, 209, 68, 13, 243, 6, 61, 5, 71, 14,
					62, 229, 8, 46, 35, 108, 118, 228, 45, 206, 124, 197, 222, 7,
				],
			),
			(
				&b"aura test payload"[..],
				[
					207, 253, 53, 34, 162, 129, 199, 102, 172, 226, 162, 199, 31, 95, 152, 172,
					210, 82, 208, 135, 33, 102, 203, 221, 252, 107, 165, 253, 201, 128, 186, 160,
				],
			),
		] {
			assert_eq!(hash_signing_payload(payload), H256::from(expected));
		}
	}

	#[test]
	fn signable_work_package_hash_signed_fields_works() {
		let (_, _, package) = crate::tests::fixture();
		let original = signable_work_package_hash(&package);

		let mutations: [(&str, fn(&mut WorkPackage)); 10] = [
			("code host", |p| p.auth_code_host += 1),
			("authorizer code", |p| p.authorizer.code_hash.0[0] ^= 1),
			("authorizer config", |p| {
				p.authorizer.config = vec![1].try_into().expect("one byte fits")
			}),
			("anchor", |p| p.context.anchor.0[0] ^= 1),
			("state root", |p| p.context.state_root.0[0] ^= 1),
			("beefy root", |p| p.context.beefy_root.0[0] ^= 1),
			("lookup anchor", |p| p.context.lookup_anchor.0[0] ^= 1),
			("lookup slot", |p| p.context.lookup_anchor_slot += 1),
			("prerequisites", |p| {
				p.context.prerequisites.insert([1; 32].into());
			}),
			("work items", |p| {
				p.items = vec![jam_types::WorkItem {
					service: 1,
					code_hash: Default::default(),
					refine_gas_limit: 1,
					accumulate_gas_limit: 1,
					export_count: 0,
					payload: Default::default(),
					import_segments: Default::default(),
					extrinsics: Default::default(),
				}]
				.try_into()
				.expect("one work item fits");
			}),
		];

		for (field, mutate) in mutations {
			let mut changed = package.clone();
			mutate(&mut changed);

			assert_ne!(signable_work_package_hash(&changed), original, "unsigned field: {field}");
		}
	}

	#[test]
	fn signable_work_package_hash_excludes_authorization_works() {
		let (_, _, mut package) = crate::tests::fixture();
		let original = signable_work_package_hash(&package);

		package.authorization = vec![1, 2, 3].try_into().expect("three bytes fit");

		assert_eq!(signable_work_package_hash(&package), original);
	}

	#[test]
	fn signable_work_package_hash_item_fields_works() {
		let (_, _, mut package) = crate::tests::fixture();

		let item = jam_types::WorkItem {
			service: 1,
			code_hash: Default::default(),
			refine_gas_limit: 1,
			accumulate_gas_limit: 1,
			export_count: 0,
			payload: Default::default(),
			import_segments: Default::default(),
			extrinsics: Default::default(),
		};

		package.items = vec![item.clone()].try_into().expect("one work item fits");
		let original = signable_work_package_hash(&package);

		let mutations: [(&str, fn(&mut jam_types::WorkItem)); 8] = [
			("service", |i| i.service += 1),
			("code hash", |i| i.code_hash.0[0] ^= 1),
			("refine gas", |i| i.refine_gas_limit += 1),
			("accumulate gas", |i| i.accumulate_gas_limit += 1),
			("export count", |i| i.export_count += 1),
			("payload", |i| i.payload = vec![1].try_into().expect("one byte fits")),
			("imports", |i| {
				i.import_segments = vec![jam_types::ImportSpec {
					root: jam_types::RootIdentifier::Direct(Default::default()),
					index: 1,
				}]
				.try_into()
				.expect("one import fits");
			}),
			("extrinsics", |i| {
				i.extrinsics = vec![jam_types::ExtrinsicSpec { hash: Default::default(), len: 1 }]
					.try_into()
					.expect("one extrinsic fits");
			}),
		];

		for (field, mutate) in mutations {
			let mut changed = item.clone();
			mutate(&mut changed);
			package.items = vec![changed].try_into().expect("one work item fits");

			assert_ne!(
				signable_work_package_hash(&package),
				original,
				"unsigned item field: {field}"
			);
		}

		let mut second = item.clone();
		second.service += 1;

		package.items = vec![item.clone(), second.clone()].try_into().expect("two work items fit");
		let ordered = signable_work_package_hash(&package);

		package.items = vec![second, item].try_into().expect("two work items fit");

		assert_ne!(signable_work_package_hash(&package), ordered);
	}
}
