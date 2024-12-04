// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Substrate runtime metadata client utilities.
//!
//! Provides simpler APIs for calling into a `frame` runtime WASM blob and
//! return metadata that needs to be checked.

use codec::{Decode, Encode};
use error::{Error, Result};
use sc_executor::WasmExecutor;
use sp_core::{
	traits::{CallContext, CodeExecutor, FetchRuntimeCode, RuntimeCode},
	OpaqueMetadata,
};
use sp_state_machine::BasicExternalities;
use sp_wasm_interface::HostFunctions;
use std::borrow::Cow;

pub mod error;

/// Fetches the latest metadata from the given runtime blob.
pub fn fetch_latest_metadata_from_code_blob<HF: HostFunctions>(
	executor: &WasmExecutor<HF>,
	code_bytes: Cow<[u8]>,
) -> Result<subxt::Metadata> {
	let runtime_caller = RuntimeCaller::new(executor, code_bytes);
	let version_result = runtime_caller.call("Metadata_metadata_versions", ());

	let opaque_metadata: OpaqueMetadata = match version_result {
		Ok(supported_versions) => {
			let supported_versions = Vec::<u32>::decode(&mut supported_versions.as_slice())
				.map_err(|e| format!("Unable to decode version list: {e}"))?;

			let latest_stable = supported_versions
				.into_iter()
				.filter(|v| *v != u32::MAX)
				.max()
				.ok_or("No stable metadata versions supported".to_string())?;

			let encoded = runtime_caller
				.call("Metadata_metadata_at_version", latest_stable)
				.map_err(|_| "Unable to fetch metadata from blob".to_string())?;

			Option::<OpaqueMetadata>::decode(&mut encoded.as_slice())
				.map_err(|_| <&str as Into<Error>>::into("Unable to decode to opaque metadata"))?
				.ok_or_else(|| "Opaque metadata not found".to_string())?
		},
		Err(_) => {
			let encoded = runtime_caller
				.call("Metadata_metadata", ())
				.map_err(|_| "Unable to fetch metadata from blob".to_string())?;
			Decode::decode(&mut encoded.as_slice())
				.map_err(|_| <&str as Into<Error>>::into("Unable to decode to opaque metadata"))?
		},
	};

	subxt::Metadata::decode(&mut (*opaque_metadata).as_slice())
		.map_err(|_| "Unable to decode opaque metadata".into())
}

pub struct BasicCodeFetcher<'a> {
	code: Cow<'a, [u8]>,
	hash: Vec<u8>,
}

impl<'a> FetchRuntimeCode for BasicCodeFetcher<'a> {
	fn fetch_runtime_code(&self) -> Option<Cow<[u8]>> {
		Some(self.code.as_ref().into())
	}
}

impl<'a> BasicCodeFetcher<'a> {
	pub fn new(code: Cow<'a, [u8]>) -> Self {
		Self { hash: sp_crypto_hashing::blake2_256(&code).to_vec(), code }
	}

	pub fn runtime_code(&'a self) -> RuntimeCode<'a> {
		RuntimeCode {
			code_fetcher: self as &'a dyn FetchRuntimeCode,
			heap_pages: None,
			hash: self.hash.clone(),
		}
	}
}

/// Simple utility that is used to call into the runtime.
pub struct RuntimeCaller<'a, 'b, HF: HostFunctions> {
	executor: &'b WasmExecutor<HF>,
	code_fetcher: BasicCodeFetcher<'a>,
}

impl<'a, 'b, HF: HostFunctions> RuntimeCaller<'a, 'b, HF> {
	pub fn new(executor: &'b WasmExecutor<HF>, code_bytes: Cow<'a, [u8]>) -> Self {
		Self { executor, code_fetcher: BasicCodeFetcher::new(code_bytes) }
	}

	pub fn call(
		&self,
		method: &str,
		data: impl Encode,
	) -> sc_executor_common::error::Result<Vec<u8>> {
		let mut ext = BasicExternalities::default();
		self.executor
			.call(
				&mut ext,
				&self.code_fetcher.runtime_code(),
				method,
				&data.encode(),
				CallContext::Offchain,
			)
			.0
	}
}

#[cfg(test)]
mod tests {
	use codec::Decode;
	use sc_executor::WasmExecutor;
	use sp_version::RuntimeVersion;

	type ParachainHostFunctions = (
		cumulus_primitives_proof_size_hostfunction::storage_proof_size::HostFunctions,
		sp_io::SubstrateHostFunctions,
	);

	#[test]
	fn test_fetch_latest_metadata_from_blob_fetches_metadata() {
		let executor: WasmExecutor<ParachainHostFunctions> = WasmExecutor::builder().build();
		let code_bytes = cumulus_test_runtime::WASM_BINARY
			.expect("To run this test, build the wasm binary of cumulus-test-runtime")
			.to_vec();
		let metadata =
			super::fetch_latest_metadata_from_code_blob(&executor, code_bytes.into()).unwrap();
		assert!(metadata.pallet_by_name("ParachainInfo").is_some());
	}

	#[test]
	fn test_runtime_caller_can_call_into_runtime() {
		let executor: WasmExecutor<ParachainHostFunctions> = WasmExecutor::builder().build();
		let code_bytes = cumulus_test_runtime::WASM_BINARY
			.expect("To run this test, build the wasm binary of cumulus-test-runtime")
			.to_vec();
		let runtime_caller = super::RuntimeCaller::new(&executor, code_bytes.into());
		let runtime_version = runtime_caller
			.call("Core_version", ())
			.expect("Should be able to call runtime_version");
		let _runtime_version: RuntimeVersion = Decode::decode(&mut runtime_version.as_slice())
			.expect("Should be able to decode runtime version");
	}
}
