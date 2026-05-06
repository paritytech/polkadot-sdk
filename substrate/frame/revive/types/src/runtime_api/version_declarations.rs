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

//! Runtime API payload-version discovery for `pallet-revive`.
//!
//! This module defines the opaque discovery payload that runtimes return to describe the latest
//! input/output payload version supported by each versioned runtime API function. Clients use it to
//! choose the newest payload version that both sides understand before making a versioned runtime
//! API call.

use alloc::{collections::BTreeMap, vec::Vec};

/// The map from versioned runtime API function names to their latest supported
/// payload version.
type RuntimeApiPayloadVersions = BTreeMap<RuntimeApiFunctionName, u32>;

/// A SCALE-friendly runtime API function name key.
type RuntimeApiFunctionName = Vec<u8>;

/// Declares the payload-version discovery type and one getter per runtime API
/// function.
macro_rules! declare_api_versions {
	(
		$(
			$function_ident:ident => ($input_payload:ty, $output_payload:ty)
		),* $(,)?
	) => {
		paste::paste! {
			/// Latest supported payload versions for all versioned `pallet-revive` runtime API
			/// functions.
			///
			/// The structure is intentionally opaque so future runtimes can add support for new
			/// functions without changing the public shape of this type. Each map key is the
			/// runtime API function name, such as `eth_transact_versioned`, and each value is the
			/// latest supported payload version for that function.
			#[derive(
				Clone,
				Debug,
				Default,
				PartialEq,
				Eq,
				PartialOrd,
				Ord,
				::codec::Encode,
				::codec::Decode,
				::scale_info::TypeInfo,
			)]
			pub struct PalletReviveRuntimeApiPayloadVersions {
				/// Function-name keyed latest supported payload versions.
				versions: RuntimeApiPayloadVersions,
			}

			impl PalletReviveRuntimeApiPayloadVersions {
				/// Returns the payload versions known by this crate at compile time.
				///
				/// # Note
				///
				/// This is hidden from generated documentation because clients must query the
				/// connected runtime instead. The returned value describes the crate version
				/// linked into the caller, which can be newer or older than the runtime being
				/// queried. Runtime code may use it to construct the discovery value that it
				/// returns from its own runtime API.
				///
				/// If you're unsure if you should use this function, questioning if it's okay to
				/// use, or questioning if it's appropriate then it means that you must not use it.
				/// Query this information from the pallet-revive runtime API instead.
				#[doc(hidden)]
				#[must_use]
				pub fn current() -> Self {
					let mut this = Self::empty();
					$(
						let input_version = <$input_payload>::LATEST_VERSION;
						let output_version = <$output_payload>::LATEST_VERSION;
						assert_eq!(
							input_version,
							output_version,
							"the input and output versions of `{}` do not match",
							stringify!($function_ident),
						);
						this = this.with_version(
							concat!(stringify!($function_ident), "_versioned"),
							input_version
						);
					)*
					this
				}

				/// Returns an empty payload-version discovery value.
				#[must_use]
				pub fn empty() -> Self {
					Self { versions: RuntimeApiPayloadVersions::new() }
				}

				/// Adds or replaces one versioned runtime API function declaration.
				#[must_use]
				pub(crate) fn with_version(
					mut self,
					runtime_api_function: impl AsRef<str>,
					version: u32,
				) -> Self {
					self.versions
						.insert(runtime_api_function.as_ref().as_bytes().to_vec(), version);
					self
				}

				$(
					/// Returns the latest supported payload version for this runtime API function.
					#[must_use]
					pub fn [<$function_ident _version>](&self) -> Option<u32> {
						self.get(concat!(stringify!($function_ident), "_versioned"))
					}
				)*

				/// Returns the latest supported payload version for a runtime API
				/// function name.
				#[must_use]
				pub fn get(&self, function_name: impl AsRef<str>) -> Option<u32> {
					self.versions.get(function_name.as_ref().as_bytes()).copied()
				}

				/// Returns whether no versioned runtime API functions are declared.
				#[must_use]
				pub fn is_empty(&self) -> bool {
					self.versions.is_empty()
				}

				/// Returns the number of versioned runtime API functions declared.
				#[must_use]
				pub fn len(&self) -> usize {
					self.versions.len()
				}
			}

			impl core::ops::Deref for PalletReviveRuntimeApiPayloadVersions {
				type Target = BTreeMap<Vec<u8>, u32>;

				fn deref(&self) -> &Self::Target {
					&self.versions
				}
			}
		}
	};
}

declare_api_versions![
	account_id => (crate::runtime_api::VersionedAccountIdInputPayload, crate::runtime_api::VersionedAccountIdOutputPayload<()>),
	address => (crate::runtime_api::VersionedAddressInputPayload<()>, crate::runtime_api::VersionedAddressOutputPayload),
	balance => (crate::runtime_api::VersionedBalanceInputPayload, crate::runtime_api::VersionedBalanceOutputPayload),
	block_author => (crate::runtime_api::VersionedBlockAuthorInputPayload, crate::runtime_api::VersionedBlockAuthorOutputPayload),
	block_gas_limit => (
		crate::runtime_api::VersionedBlockGasLimitInputPayload,
		crate::runtime_api::VersionedBlockGasLimitOutputPayload
	),
	call => (crate::runtime_api::VersionedCallInputPayload<(), ()>, crate::runtime_api::VersionedCallOutputPayload<()>),
	code => (crate::runtime_api::VersionedCodeInputPayload, crate::runtime_api::VersionedCodeOutputPayload),
	eth_block => (crate::runtime_api::VersionedEthBlockInputPayload, crate::runtime_api::VersionedEthBlockOutputPayload),
	eth_block_hash => (
		crate::runtime_api::VersionedEthBlockHashInputPayload,
		crate::runtime_api::VersionedEthBlockHashOutputPayload
	),
	eth_estimate_gas => (
		crate::runtime_api::VersionedEthEstimateGasInputPayload<()>,
		crate::runtime_api::VersionedEthEstimateGasOutputPayload
	),
	eth_pre_dispatch_weight => (
		crate::runtime_api::VersionedEthPreDispatchWeightInputPayload,
		crate::runtime_api::VersionedEthPreDispatchWeightOutputPayload
	),
	eth_receipt_data => (
		crate::runtime_api::VersionedEthReceiptDataInputPayload,
		crate::runtime_api::VersionedEthReceiptDataOutputPayload
	),
	eth_transact => (
		crate::runtime_api::VersionedEthTransactInputPayload<()>,
		crate::runtime_api::VersionedEthTransactOutputPayload<()>
	),
	gas_price => (crate::runtime_api::VersionedGasPriceInputPayload, crate::runtime_api::VersionedGasPriceOutputPayload),
	get_storage => (crate::runtime_api::VersionedGetStorageInputPayload, crate::runtime_api::VersionedGetStorageOutputPayload),
	instantiate => (
		crate::runtime_api::VersionedInstantiateInputPayload<(), ()>,
		crate::runtime_api::VersionedInstantiateOutputPayload<()>
	),
	max_extrinsic_weight_in_gas => (
		crate::runtime_api::VersionedMaxExtrinsicWeightInGasInputPayload,
		crate::runtime_api::VersionedMaxExtrinsicWeightInGasOutputPayload
	),
	new_balance_with_dust => (
		crate::runtime_api::VersionedNewBalanceWithDustInputPayload,
		crate::runtime_api::VersionedNewBalanceWithDustOutputPayload<()>
	),
	nonce => (crate::runtime_api::VersionedNonceInputPayload, crate::runtime_api::VersionedNonceOutputPayload<()>),
	runtime_pallets_address => (
		crate::runtime_api::VersionedRuntimePalletsAddressInputPayload,
		crate::runtime_api::VersionedRuntimePalletsAddressOutputPayload
	),
	trace_block => (crate::runtime_api::VersionedTraceBlockInputPayload, crate::runtime_api::VersionedTraceBlockOutputPayload),
	trace_call => (crate::runtime_api::VersionedTraceCallInputPayload, crate::runtime_api::VersionedTraceCallOutputPayload),
	trace_tx => (crate::runtime_api::VersionedTraceTxInputPayload, crate::runtime_api::VersionedTraceTxOutputPayload),
	upload_code => (
		crate::runtime_api::VersionedUploadCodeInputPayload<(), ()>,
		crate::runtime_api::VersionedUploadCodeOutputPayload<()>
	),
];

#[cfg(test)]
mod tests {
	use alloc::{collections::BTreeMap, vec::Vec};

	use super::PalletReviveRuntimeApiPayloadVersions;

	#[test]
	fn empty_declaration_contains_no_versions() {
		// Arrange
		let versions = PalletReviveRuntimeApiPayloadVersions::empty();

		// Act
		let version = versions.eth_transact_version();

		// Assert
		assert!(versions.is_empty());
		assert_eq!(versions.len(), 0);
		assert_eq!(version, None);
	}

	#[test]
	fn current_declaration_contains_all_known_interface_versions() {
		// Arrange
		let versions = PalletReviveRuntimeApiPayloadVersions::current();

		// Act
		let eth_transact_version = versions.eth_transact_version();
		let trace_call_version = versions.trace_call_version();
		let upload_code_version = versions.upload_code_version();

		// Assert
		assert_eq!(versions.len(), 24);
		assert_eq!(eth_transact_version, Some(1));
		assert_eq!(trace_call_version, Some(1));
		assert_eq!(upload_code_version, Some(1));
	}

	#[test]
	fn generic_lookup_uses_versioned_runtime_api_function_names() {
		// Arrange
		let versions = PalletReviveRuntimeApiPayloadVersions::current();

		// Act
		let runtime_api_name = versions.get("eth_transact");
		let versioned_name = versions.get("eth_transact_versioned");

		// Assert
		assert_eq!(runtime_api_name, None);
		assert_eq!(versioned_name, Some(1));
	}

	#[test]
	fn declaration_derefs_to_function_version_map() {
		// Arrange
		let versions = PalletReviveRuntimeApiPayloadVersions::current();

		// Act
		let map: &BTreeMap<Vec<u8>, u32> = &versions;

		// Assert
		assert_eq!(map.get(b"eth_transact_versioned".as_slice()), Some(&1));
		assert_eq!(map.get(b"eth_transact".as_slice()), None);
	}
}
