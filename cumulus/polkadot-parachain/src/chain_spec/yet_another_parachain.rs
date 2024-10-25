// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! ChainSpecs dedicated to parachain setups for testing and example purposes

use crate::chain_spec::get_from_seed;
use parachains_common::genesis_config_helpers::get_account_id_from_seed;
use polkadot_omni_node_lib::chain_spec::{Extensions, GenericChainSpec};
use sc_chain_spec::ChainType;
use sp_core::{
	sr25519::{self, Pair as SrPair},
	Pair,
};
use yet_another_parachain_runtime::AuraId;

const NUM_ACCOUNT_PAIRS: usize = 16000;

fn derive_accounts(n: usize, seed: String) -> Vec<SrPair> {
	let t = std::cmp::min(
		n,
		std::thread::available_parallelism().unwrap_or(1usize.try_into().unwrap()).get(),
	);

	let mut tn = (0..t).cycle();
	let mut tranges: Vec<_> = (0..t).map(|_| Vec::new()).collect();
	(0..n).for_each(|i| tranges[tn.next().unwrap()].push(i));
	let mut threads = Vec::new();

	tranges.into_iter().for_each(|chunk| {
		let seed = seed.clone();
		threads.push(std::thread::spawn(move || {
			chunk
				.into_iter()
				.map(move |i| {
					let derivation = format!("{seed}/{i}");
					// get_account_id_from_seed::<sr25519::Public>(&derivation)
					<SrPair as Pair>::from_string(&derivation, None).unwrap()
				})
				.collect::<Vec<_>>()
		}));
	});

	threads
		.into_iter()
		.map(|h| h.join().unwrap())
		.flatten()
		// .map(|p| (p, funds))
		.collect()
}

pub fn yet_another_parachain_config(
	relay: impl Into<String>,
	chain_type: ChainType,
	para_id: u32,
) -> GenericChainSpec {
	let mut endowed_accounts = vec![
		get_account_id_from_seed::<sr25519::Public>("Alice"),
		get_account_id_from_seed::<sr25519::Public>("Bob"),
		get_account_id_from_seed::<sr25519::Public>("Charlie"),
		get_account_id_from_seed::<sr25519::Public>("Dave"),
		get_account_id_from_seed::<sr25519::Public>("Eve"),
		get_account_id_from_seed::<sr25519::Public>("Ferdie"),
		get_account_id_from_seed::<sr25519::Public>("Alice//stash"),
		get_account_id_from_seed::<sr25519::Public>("Bob//stash"),
		get_account_id_from_seed::<sr25519::Public>("Charlie//stash"),
		get_account_id_from_seed::<sr25519::Public>("Dave//stash"),
		get_account_id_from_seed::<sr25519::Public>("Eve//stash"),
		get_account_id_from_seed::<sr25519::Public>("Ferdie//stash"),
	];

	endowed_accounts.extend(
		derive_accounts(NUM_ACCOUNT_PAIRS, "//Sender".into())
			.into_iter()
			.map(|k| k.public().into()),
	);
	endowed_accounts.extend(
		derive_accounts(NUM_ACCOUNT_PAIRS, "//Receiver".into())
			.into_iter()
			.map(|k| k.public().into()),
	);

	GenericChainSpec::builder(
		yet_another_parachain_runtime::WASM_BINARY
			.expect("WASM binary was not built, please build it!"),
		Extensions { relay_chain: relay.into(), para_id },
	)
	.with_name("Yet Another Parachain")
	.with_id("yet_another_parachain")
	.with_chain_type(chain_type)
	.with_genesis_config_patch(serde_json::json!({
		"balances": {
			"balances": endowed_accounts.iter().cloned().map(|k| (k, 1u64 << 60)).collect::<Vec<_>>(),
		},
		"sudo": { "key": Some(get_account_id_from_seed::<sr25519::Public>("Alice")) },
		"parachainInfo": {
			"parachainId": para_id,
		},
		"aura": { "authorities": vec![get_from_seed::<AuraId>("Alice"), get_from_seed::<AuraId>("Bob")] },
	}))
	.build()
}
