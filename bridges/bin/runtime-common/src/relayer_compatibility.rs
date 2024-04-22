// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! An attempt to implement bridge versioning and make relayer (an offchain actor)
//! compatible with different runtime versions, implemeting the same bridge version.
//!
//! Before this versioning, we had to prepare + deploy a new relay version for every
//! runtime upgrade. And since relay is connected to 4 chains, we need to have a new
//! version every time one of chains is upgraded. Recently we have "eased" our
//! requirements and now only watch for bridge hub versions (2 chains).
//!
//! What we are trying to solve with that:
//!
//! - transaction encoding compatibility - when `spec_version`, `transaction_version`, our pallet
//!   calls encoding, pallet ordering or a set of signed extension changes, relay starts building
//!   invalid transactions. For first two things we have a CLI options to read versions right from
//!   runtime. The rest should happen rarely;
//!
//! - bridge configuration compatibility. E.g. relayer compensation/reward scheme may change over
//!   time. And relayer may everntually start losing its tokens by submitting valid and previously
//!   compensated transactions;
//!
//! - unexpected/unknown issues. E.g. : change of `paras` pallet at the relay chain, storage trie
//!   version changes, ... That is something that we expect needs to be detected by
//!   zombienet/chopsticks tests in the fellowhsip repo. But so far it isn't automated and normally
//!   when we are building new relay version for upgraded chains, we need to run Add P<>K bridge
//!   manual zombienet test for asset transfer polkadot-fellows/runtimes#198 and at least detect it
//!   manually.
//!
//! TLDR: by releasing a new relayer version on every runtime upgrade we were trying to ensure
//! that everything works properly. If we ever have an automated testing for that, we will be
//! able to solve that easier. Yet we have an issue, to make relayer fully generic, but it is
//! unlikely to be finished soon.
//!
//! What we can do to make our lives easier now and not to rebuild relayer on every upgrade.
//! Inspired by how pallet storage versioning works: we can add some constant to every bridge
//! pallet (GRANPA, parachains, messages) configuration and also add the same constant to relayer.
//! Relayer should exit if it sees that the constant it has is different from the constant in the
//! runtime. This should solve issues (1) and (2) from above. (3) is still actual and can't be
//! detected without having automated integration tests.
//!
//! This constant should 'seal' everything related to bridge transaction encoding and bridge
//! configuration compatibility:
//!
//! - a set of bridge calls encoding;
//!
//! - a set of bridge-related signed extensions IDs that are related to the bridge;
//!
//! - a set of all pallet settings that may affect relayer.

use bp_polkadot_core::parachains::ParaHeadsProof;
use bp_runtime::RelayerVersion;
use bp_test_utils::make_default_justification;
use codec::Encode;
use pallet_bridge_grandpa::{
	BridgedHeader as BridgedGrandpaHeader, Call as GrandpaCall, Config as GrandpaConfig,
};
use pallet_bridge_parachains::{Call as ParachainsCall, Config as ParachainsConfig};
use sp_core::{blake2_256, Get, H256};
use sp_runtime::traits::{Header, SignedExtension};

const LOG_TARGET: &str = "bridge";

/// A set of signed extensions that are:
///
/// - not related to bridge operations;
///
/// - have the `AdditionalSigned` set to `()`, so it doesn't break the bridge relayer.
const IGNORED_EXTENSIONS: [&'static str; 0] = [];

/// Ensure that the running relayer is compatible with the `pallet-bridge-grandpa`, deployed
/// at `Runtime`.
pub fn ensure_grandpa_relayer_compatibility<Runtime, I, SignedExtra>()
where
	Runtime: GrandpaConfig<I>,
	I: 'static,
	SignedExtra: SignedExtension,
	Runtime::RuntimeCall: From<GrandpaCall<Runtime, I>>,
{
	let expected_version = Runtime::CompatibleWithRelayer::get();
	let actual_version = RelayerVersion {
		manual: expected_version.manual,
		auto: blake2_256(
			&[grandpa_calls_digest::<Runtime, I>(), siged_extensions_digest::<SignedExtra>()]
				.encode(),
		)
		.into(),
	};
	assert_eq!(
		expected_version, actual_version,
		"Expected GRANDPA relayer version: {expected_version:?}. Actual: {actual_version:?}",
	);
}

/// Ensure that the running relayer is compatible with the `pallet-bridge-parachains`, deployed
/// at `Runtime`.
pub fn ensure_parachains_relayer_compatibility<Runtime, I, SignedExtra>()
where
	Runtime: ParachainsConfig<I>,
	I: 'static,
	SignedExtra: SignedExtension,
	Runtime::RuntimeCall: From<ParachainsCall<Runtime, I>>,
{
	let expected_version = <Runtime as ParachainsConfig<I>>::CompatibleWithRelayer::get();
	let actual_version = RelayerVersion {
		manual: expected_version.manual,
		auto: blake2_256(
			&[parachains_calls_digest::<Runtime, I>(), siged_extensions_digest::<SignedExtra>()]
				.encode(),
		)
		.into(),
	};
	assert_eq!(
		expected_version, actual_version,
		"Expected parachains relayer version: {expected_version:?}. Actual: {actual_version:?}",
	);
}

/// Seal bridge GRANDPA call encoding.
fn grandpa_calls_digest<Runtime, I>() -> H256
where
	Runtime: GrandpaConfig<I>,
	I: 'static,
	Runtime::RuntimeCall: From<GrandpaCall<Runtime, I>>,
{
	// the relayer normally only uses the `submit_finality_proof` call. Let's ensure that
	// the encoding stays the same. Obviously, we can not detect all encoding changes here,
	// but such breaking changes are not assumed to be detected using this test.
	let bridged_header = BridgedGrandpaHeader::<Runtime, I>::new(
		42u32.into(),
		Default::default(),
		Default::default(),
		Default::default(),
		Default::default(),
	);
	let bridged_justification = make_default_justification(&bridged_header);
	let call: Runtime::RuntimeCall = GrandpaCall::<Runtime, I>::submit_finality_proof {
		finality_target: Box::new(bridged_header),
		justification: bridged_justification,
	}
	.into();
	log::info!(target: LOG_TARGET, "Sealing GRANDPA call encoding: {:?}", call);
	blake2_256(&call.encode()).into()
}

/// Seal bridge parachains call encoding.
fn parachains_calls_digest<Runtime, I>() -> H256
where
	Runtime: ParachainsConfig<I>,
	I: 'static,
	Runtime::RuntimeCall: From<ParachainsCall<Runtime, I>>,
{
	// the relayer normally only uses the `submit_parachain_heads` call. Let's ensure that
	// the encoding stays the same. Obviously, we can not detect all encoding changes here,
	// but such breaking changes are not assumed to be detected using this test.
	let call: Runtime::RuntimeCall = ParachainsCall::<Runtime, I>::submit_parachain_heads {
		at_relay_block: (84, Default::default()),
		parachains: vec![(42.into(), Default::default())],
		parachain_heads_proof: ParaHeadsProof { storage_proof: vec![vec![42u8; 42]] },
	}
	.into();
	log::info!(target: LOG_TARGET, "Sealing parachains call encoding: {:?}", call);
	blake2_256(&call.encode()).into()
}

/// Seal all signed extensions that may break bridge.
fn siged_extensions_digest<SignedExtra: SignedExtension>() -> H256 {
	let extensions: Vec<_> = SignedExtra::metadata()
		.into_iter()
		.map(|m| m.identifier)
		.filter(|id| !IGNORED_EXTENSIONS.contains(id))
		.collect();
	log::info!(target: LOG_TARGET, "Sealing runtime extensions: {:?}", extensions);
	blake2_256(&extensions.encode()).into()
}
