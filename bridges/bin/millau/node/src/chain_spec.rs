// Copyright 2019-2021 Parity Technologies (UK) Ltd.
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

use millau_runtime::{
	AccountId, AuraConfig, BalancesConfig, BeefyConfig, BridgeRialtoMessagesConfig,
	BridgeRialtoParachainMessagesConfig, BridgeWestendGrandpaConfig, GrandpaConfig,
	RuntimeGenesisConfig, SessionConfig, SessionKeys, Signature, SudoConfig, SystemConfig,
	WASM_BINARY,
};
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_consensus_beefy::crypto::AuthorityId as BeefyId;
use sp_consensus_grandpa::AuthorityId as GrandpaId;
use sp_core::{sr25519, Pair, Public};
use sp_runtime::traits::{IdentifyAccount, Verify};

/// "Names" of the authorities accounts at local testnet.
const LOCAL_AUTHORITIES_ACCOUNTS: [&str; 5] = ["Alice", "Bob", "Charlie", "Dave", "Eve"];
/// "Names" of the authorities accounts at development testnet.
const DEV_AUTHORITIES_ACCOUNTS: [&str; 1] = [LOCAL_AUTHORITIES_ACCOUNTS[0]];
/// "Names" of all possible authorities accounts.
const ALL_AUTHORITIES_ACCOUNTS: [&str; 5] = LOCAL_AUTHORITIES_ACCOUNTS;
/// "Name" of the `sudo` account.
const SUDO_ACCOUNT: &str = "Sudo";
/// "Name" of the account, which owns the with-Westend GRANDPA pallet.
const WESTEND_GRANDPA_PALLET_OWNER: &str = "Westend.GrandpaOwner";
/// "Name" of the account, which owns the with-Rialto messages pallet.
const RIALTO_MESSAGES_PALLET_OWNER: &str = "Rialto.MessagesOwner";
/// "Name" of the account, which owns the with-RialtoParachain messages pallet.
const RIALTO_PARACHAIN_MESSAGES_PALLET_OWNER: &str = "RialtoParachain.MessagesOwner";

/// Specialized `ChainSpec`. This is a specialization of the general Substrate ChainSpec type.
pub type ChainSpec = sc_service::GenericChainSpec<RuntimeGenesisConfig>;

/// The chain specification option. This is expected to come in from the CLI and
/// is little more than one of a number of alternatives which can easily be converted
/// from a string (`--chain=...`) into a `ChainSpec`.
#[derive(Clone, Debug)]
pub enum Alternative {
	/// Whatever the current runtime is, with just Alice as an auth.
	Development,
	/// Whatever the current runtime is, with simple Alice/Bob/Charlie/Dave/Eve auths.
	LocalTestnet,
}

/// Helper function to generate a crypto pair from seed
pub fn get_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
	TPublic::Pair::from_string(&format!("//{seed}"), None)
		.expect("static values are valid; qed")
		.public()
}

type AccountPublic = <Signature as Verify>::Signer;

/// Helper function to generate an account ID from seed
pub fn get_account_id_from_seed<TPublic: Public>(seed: &str) -> AccountId
where
	AccountPublic: From<<TPublic::Pair as Pair>::Public>,
{
	AccountPublic::from(get_from_seed::<TPublic>(seed)).into_account()
}

/// Helper function to generate an authority key for Aura
pub fn get_authority_keys_from_seed(s: &str) -> (AccountId, AuraId, BeefyId, GrandpaId) {
	(
		get_account_id_from_seed::<sr25519::Public>(s),
		get_from_seed::<AuraId>(s),
		get_from_seed::<BeefyId>(s),
		get_from_seed::<GrandpaId>(s),
	)
}

impl Alternative {
	/// Get an actual chain config from one of the alternatives.
	pub(crate) fn load(self) -> ChainSpec {
		let properties = Some(
			serde_json::json!({
				"tokenDecimals": 9,
				"tokenSymbol": "MLAU"
			})
			.as_object()
			.expect("Map given; qed")
			.clone(),
		);
		match self {
			Alternative::Development => ChainSpec::from_genesis(
				"Millau Development",
				"millau_dev",
				sc_service::ChainType::Development,
				|| {
					testnet_genesis(
						DEV_AUTHORITIES_ACCOUNTS
							.into_iter()
							.map(get_authority_keys_from_seed)
							.collect(),
						get_account_id_from_seed::<sr25519::Public>(SUDO_ACCOUNT),
						endowed_accounts(),
						true,
					)
				},
				vec![],
				None,
				None,
				None,
				properties,
				None,
			),
			Alternative::LocalTestnet => ChainSpec::from_genesis(
				"Millau Local",
				"millau_local",
				sc_service::ChainType::Local,
				|| {
					testnet_genesis(
						LOCAL_AUTHORITIES_ACCOUNTS
							.into_iter()
							.map(get_authority_keys_from_seed)
							.collect(),
						get_account_id_from_seed::<sr25519::Public>(SUDO_ACCOUNT),
						endowed_accounts(),
						true,
					)
				},
				vec![],
				None,
				None,
				None,
				properties,
				None,
			),
		}
	}
}

/// We're using the same set of endowed accounts on all Millau chains (dev/local) to make
/// sure that all accounts, required for bridge to be functional (e.g. relayers fund account,
/// accounts used by relayers in our test deployments, accounts used for demonstration
/// purposes), are all available on these chains.
fn endowed_accounts() -> Vec<AccountId> {
	let all_authorities = ALL_AUTHORITIES_ACCOUNTS.iter().flat_map(|x| {
		[
			get_account_id_from_seed::<sr25519::Public>(x),
			get_account_id_from_seed::<sr25519::Public>(&format!("{x}//stash")),
		]
	});
	vec![
		// Sudo account
		get_account_id_from_seed::<sr25519::Public>(SUDO_ACCOUNT),
		// Regular (unused) accounts
		get_account_id_from_seed::<sr25519::Public>("Ferdie"),
		get_account_id_from_seed::<sr25519::Public>("Ferdie//stash"),
		// Accounts, used by Westend<>Millau bridge
		get_account_id_from_seed::<sr25519::Public>(WESTEND_GRANDPA_PALLET_OWNER),
		get_account_id_from_seed::<sr25519::Public>("Westend.HeadersRelay1"),
		get_account_id_from_seed::<sr25519::Public>("Westend.HeadersRelay2"),
		get_account_id_from_seed::<sr25519::Public>("Westend.AssetHubWestendHeaders1"),
		get_account_id_from_seed::<sr25519::Public>("Westend.AssetHubWestendHeaders2"),
		// Accounts, used by Rialto<>Millau bridge
		get_account_id_from_seed::<sr25519::Public>(RIALTO_MESSAGES_PALLET_OWNER),
		get_account_id_from_seed::<sr25519::Public>("Rialto.HeadersAndMessagesRelay"),
		get_account_id_from_seed::<sr25519::Public>("Rialto.OutboundMessagesRelay.Lane00000001"),
		get_account_id_from_seed::<sr25519::Public>("Rialto.InboundMessagesRelay.Lane00000001"),
		get_account_id_from_seed::<sr25519::Public>("Rialto.MessagesSender"),
		// Accounts, used by RialtoParachain<>Millau bridge
		get_account_id_from_seed::<sr25519::Public>(RIALTO_PARACHAIN_MESSAGES_PALLET_OWNER),
		get_account_id_from_seed::<sr25519::Public>("RialtoParachain.HeadersAndMessagesRelay1"),
		get_account_id_from_seed::<sr25519::Public>("RialtoParachain.HeadersAndMessagesRelay2"),
		get_account_id_from_seed::<sr25519::Public>("RialtoParachain.RialtoHeadersRelay1"),
		get_account_id_from_seed::<sr25519::Public>("RialtoParachain.RialtoHeadersRelay2"),
		get_account_id_from_seed::<sr25519::Public>("RialtoParachain.MessagesSender"),
	]
	.into_iter()
	.chain(all_authorities)
	.collect()
}

fn session_keys(aura: AuraId, beefy: BeefyId, grandpa: GrandpaId) -> SessionKeys {
	SessionKeys { aura, beefy, grandpa }
}

fn testnet_genesis(
	initial_authorities: Vec<(AccountId, AuraId, BeefyId, GrandpaId)>,
	root_key: AccountId,
	endowed_accounts: Vec<AccountId>,
	_enable_println: bool,
) -> RuntimeGenesisConfig {
	RuntimeGenesisConfig {
		system: SystemConfig {
			code: WASM_BINARY.expect("Millau development WASM not available").to_vec(),
			..Default::default()
		},
		balances: BalancesConfig {
			balances: endowed_accounts.iter().cloned().map(|k| (k, 1 << 50)).collect(),
		},
		aura: AuraConfig { authorities: Vec::new() },
		beefy: BeefyConfig::default(),
		grandpa: GrandpaConfig { authorities: Vec::new(), ..Default::default() },
		sudo: SudoConfig { key: Some(root_key) },
		session: SessionConfig {
			keys: initial_authorities
				.iter()
				.map(|x| {
					(x.0.clone(), x.0.clone(), session_keys(x.1.clone(), x.2.clone(), x.3.clone()))
				})
				.collect::<Vec<_>>(),
		},
		bridge_westend_grandpa: BridgeWestendGrandpaConfig {
			// for our deployments to avoid multiple same-nonces transactions:
			// //Alice is already used to initialize Rialto<->Millau bridge
			// => let's use //Westend.GrandpaOwner to initialize Westend->Millau bridge
			owner: Some(get_account_id_from_seed::<sr25519::Public>(WESTEND_GRANDPA_PALLET_OWNER)),
			..Default::default()
		},
		bridge_rialto_messages: BridgeRialtoMessagesConfig {
			owner: Some(get_account_id_from_seed::<sr25519::Public>(RIALTO_MESSAGES_PALLET_OWNER)),
			..Default::default()
		},
		bridge_rialto_parachain_messages: BridgeRialtoParachainMessagesConfig {
			owner: Some(get_account_id_from_seed::<sr25519::Public>(
				RIALTO_PARACHAIN_MESSAGES_PALLET_OWNER,
			)),
			..Default::default()
		},
		xcm_pallet: Default::default(),
	}
}
