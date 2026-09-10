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

//! Command line companion of [`pallet_bootnodes`].
//!
//! - `inspect` reads the pallet's constants and its bootnode set at the latest (or a given) block
//!   of a running node.
//! - `sign` derives the node key from the node's secret key and prints a signed bootnode entry,
//!   both SCALE-encoded for `add_bootnode` and in the JSON form a chain spec's genesis config
//!   takes.
//!
//! The pallet is located through the runtime metadata, so the tool needs nothing but an RPC
//! endpoint; `--para-id` and `--jam-genesis-hash` can override or, with `--offline`, replace what
//! would be read from the chain.
//!
//! [`Command`] is a `clap` parser, so a node binary can embed it as a subcommand and hand it to
//! [`run`].

use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use codec::{Decode, Encode};
use frame_metadata::{RuntimeMetadata, RuntimeMetadataPrefixed};
use jsonrpsee::{
	core::{client::ClientT, params::ArrayParams},
	http_client::{HttpClient, HttpClientBuilder},
	rpc_params,
	ws_client::{WsClient, WsClientBuilder},
};
use multiaddr::Multiaddr;
use pallet_bootnodes::{multiaddr as addr_check, pop_payload, Bootnode};
use sc_network_types::PeerId;
use serde::{de::DeserializeOwned, Serialize};
use sp_core::{bounded::BoundedVec, crypto::Pair as _, ed25519, Bytes, ConstU32, H256};
use sp_crypto_hashing::twox_128;
use std::{collections::BTreeMap, path::PathBuf};
use zeroize::Zeroizing;

/// A bootnode entry as read from chain.
///
/// The address bound only matters when the runtime accepts an entry, so the widest possible one
/// is used for decoding.
pub type StoredBootnode = Bootnode<ConstU32<{ u32::MAX }>>;

/// Inspect `pallet-bootnodes` and sign bootnode entries.
#[derive(Debug, Parser)]
#[command(name = "bootnodes-cli", version)]
pub struct Command {
	#[command(subcommand)]
	pub sub: Sub,
}

#[derive(Debug, Subcommand)]
pub enum Sub {
	/// Print the pallet's constants and bootnode set.
	Inspect(InspectCmd),
	/// Produce a signed bootnode entry for this node, to be submitted with `add_bootnode`.
	Sign(SignCmd),
}

/// How to reach the chain.
#[derive(Debug, Args)]
pub struct ChainArgs {
	/// RPC endpoint of a node of the parachain (`ws://`, `wss://`, `http://` or `https://`).
	#[arg(long, default_value = "ws://127.0.0.1:9944")]
	pub url: String,
	/// Block hash to read state at. Defaults to the latest best block.
	#[arg(long)]
	pub at: Option<H256>,
	/// Name of the pallet in the runtime, as given to `construct_runtime`.
	#[arg(long, default_value = "Bootnodes")]
	pub pallet: String,
}

#[derive(Debug, Args)]
pub struct InspectCmd {
	#[command(flatten)]
	pub chain: ChainArgs,
}

#[derive(Debug, Args)]
pub struct SignCmd {
	#[command(flatten)]
	pub chain: ChainArgs,
	/// The node's public multiaddr: `/(ip4|ip6|dns|dns4|dns6)/<host>/tcp/<port>`, optionally
	/// followed by `/ws` or `/wss`, without a `/p2p/` component.
	#[arg(long)]
	pub addr: String,
	/// The node's ed25519 secret key as 64 hex characters (what the node gets as `--node-key`).
	///
	/// Prefer `--node-key-file`: a key passed here is recorded in the shell history and readable
	/// by any process that can list this process's command line.
	#[arg(long, required_unless_present = "node_key_file", conflicts_with = "node_key_file")]
	pub node_key: Option<String>,
	/// File holding the node's ed25519 secret key, as hex or as 32 raw bytes (what the node gets
	/// as `--node-key-file`, or its `<base-path>/chains/<chain-id>/network/secret_ed25519`).
	#[arg(long)]
	pub node_key_file: Option<PathBuf>,
	/// JAM timeslot after which readers drop the entry. A consent lease: pick a slot far enough
	/// ahead to cover the time until the entry is re-signed and re-submitted.
	#[arg(long)]
	pub expiry_slot: u32,
	/// Para id to sign for. Defaults to the pallet's `ParaId` constant.
	#[arg(long)]
	pub para_id: Option<u32>,
	/// JAM genesis hash to sign for. Defaults to the pallet's `JamGenesisHash` constant.
	#[arg(long)]
	pub jam_genesis_hash: Option<H256>,
	/// Do not contact the chain. Requires `--para-id` and `--jam-genesis-hash`.
	#[arg(long, requires_all = ["para_id", "jam_genesis_hash"])]
	pub offline: bool,
}

/// Run a parsed [`Command`], printing its JSON result to stdout.
pub async fn run(cmd: Command) -> Result<()> {
	match cmd.sub {
		Sub::Inspect(cmd) => inspect(cmd).await,
		Sub::Sign(cmd) => sign(cmd).await,
	}
}

async fn inspect(cmd: InspectCmd) -> Result<()> {
	let chain = Chain::connect(&cmd.chain).await?;
	let bootnodes = chain.bootnodes().await?;

	let out = InspectOut {
		at: chain.at,
		pallet: cmd.chain.pallet,
		para_id: chain.constant("ParaId")?,
		jam_genesis_hash: chain.constant("JamGenesisHash")?,
		max_bootnodes: chain.constant("MaxBootnodes")?,
		max_addr_len: chain.constant("MaxAddrLen")?,
		bootnodes: bootnodes.iter().map(BootnodeOut::from).collect(),
	};
	println!("{}", serde_json::to_string_pretty(&out)?);
	Ok(())
}

async fn sign(cmd: SignCmd) -> Result<()> {
	let seed = load_node_key(&cmd)?;
	let pair = ed25519::Pair::from_seed(&seed);
	let addr = parse_addr(&cmd.addr)?;

	let (at, para_id, jam_genesis_hash) = if cmd.offline {
		// `clap` enforces the presence of the two flags.
		let missing = |flag: &str| anyhow!("--offline requires {flag}");
		(
			None,
			cmd.para_id.ok_or_else(|| missing("--para-id"))?,
			cmd.jam_genesis_hash.ok_or_else(|| missing("--jam-genesis-hash"))?,
		)
	} else {
		let chain = Chain::connect(&cmd.chain).await?;
		let para_id = match cmd.para_id {
			Some(para_id) => para_id,
			None => chain.constant("ParaId")?,
		};
		let jam_genesis_hash = match cmd.jam_genesis_hash {
			Some(hash) => hash,
			None => chain.constant("JamGenesisHash")?,
		};
		let max_addr_len: u32 = chain.constant("MaxAddrLen")?;
		if addr.len() > max_addr_len as usize {
			bail!(
				"the address encodes to {} bytes, the runtime allows at most {max_addr_len}",
				addr.len()
			);
		}
		(Some(chain.at), para_id, jam_genesis_hash)
	};

	let bootnode = sign_bootnode(&pair, &addr, para_id, &jam_genesis_hash, cmd.expiry_slot);
	let out = SignOut::new(at, para_id, jam_genesis_hash, &addr, bootnode);
	println!("{}", serde_json::to_string_pretty(&out)?);
	Ok(())
}

/// Sign `addr` with `pair` as a bootnode of `para_id` on the JAM chain `jam_genesis_hash`, listed
/// until `expiry_slot`.
fn sign_bootnode(
	pair: &ed25519::Pair,
	addr: &Multiaddr,
	para_id: u32,
	jam_genesis_hash: &H256,
	expiry_slot: u32,
) -> StoredBootnode {
	let addr = addr.to_vec();
	let sig = pair.sign(&pop_payload(jam_genesis_hash, para_id, &addr, expiry_slot));
	StoredBootnode {
		addr: BoundedVec::truncate_from(addr),
		node_key: pair.public(),
		expiry_slot,
		sig,
	}
}

#[derive(Serialize)]
struct InspectOut {
	at: H256,
	pallet: String,
	para_id: u32,
	jam_genesis_hash: H256,
	max_bootnodes: u32,
	max_addr_len: u32,
	bootnodes: Vec<BootnodeOut>,
}

#[derive(Serialize)]
struct BootnodeOut {
	node_key: String,
	peer_id: Option<String>,
	addr: String,
	/// The address with the peer id appended, as accepted by a node's `--bootnodes` flag.
	dial: Option<String>,
	expiry_slot: u32,
	sig: String,
}

impl From<&StoredBootnode> for BootnodeOut {
	fn from(b: &StoredBootnode) -> Self {
		let peer_id = peer_id(&b.node_key);
		let addr = Multiaddr::try_from(b.addr.to_vec()).ok();
		Self {
			node_key: hex(b.node_key.as_ref()),
			peer_id: peer_id.map(|p| p.to_string()),
			addr: addr.as_ref().map(ToString::to_string).unwrap_or_else(|| hex(&b.addr)),
			dial: addr.zip(peer_id).map(|(addr, peer)| format!("{addr}/p2p/{peer}")),
			expiry_slot: b.expiry_slot,
			sig: hex(b.sig.as_ref()),
		}
	}
}

#[derive(Serialize)]
struct SignOut {
	/// Block the constants were read at; absent with `--offline`.
	at: Option<H256>,
	para_id: u32,
	jam_genesis_hash: H256,
	node_key: String,
	peer_id: Option<String>,
	addr: String,
	expiry_slot: u32,
	sig: String,
	/// SCALE encoding of the `Bootnode`, the argument of `add_bootnode`.
	encoded_bootnode: String,
	/// The entry in the JSON form the pallet's genesis config takes: an element of the
	/// `bootnodes` list in a chain spec.
	bootnode: StoredBootnode,
}

impl SignOut {
	fn new(
		at: Option<H256>,
		para_id: u32,
		jam_genesis_hash: H256,
		addr: &Multiaddr,
		bootnode: StoredBootnode,
	) -> Self {
		Self {
			at,
			para_id,
			jam_genesis_hash,
			node_key: hex(bootnode.node_key.as_ref()),
			peer_id: peer_id(&bootnode.node_key).map(|p| p.to_string()),
			addr: addr.to_string(),
			expiry_slot: bootnode.expiry_slot,
			sig: hex(bootnode.sig.as_ref()),
			encoded_bootnode: hex(&bootnode.encode()),
			bootnode,
		}
	}
}

fn hex(bytes: &[u8]) -> String {
	array_bytes::bytes2hex("0x", bytes)
}

fn peer_id(node_key: &ed25519::Public) -> Option<PeerId> {
	PeerId::from_ed25519(&<[u8; 32]>::from(*node_key))
}

/// Decode a 32 byte key given as 64 hex characters, with or without a `0x` prefix.
fn decode_hex_key(s: &str) -> Option<Vec<u8>> {
	let s = s.trim();
	let s = s.strip_prefix("0x").unwrap_or(s);
	if s.len() != 64 {
		return None;
	}
	array_bytes::hex2bytes(s).ok()
}

/// Load the node's ed25519 secret key from the `--node-key` or `--node-key-file` argument.
fn load_node_key(cmd: &SignCmd) -> Result<Zeroizing<[u8; 32]>> {
	let bytes = match (&cmd.node_key, &cmd.node_key_file) {
		(Some(hex), None) => Zeroizing::new(
			decode_hex_key(hex).ok_or_else(|| anyhow!("--node-key must be 64 hex characters"))?,
		),
		(None, Some(path)) => {
			let raw = Zeroizing::new(
				std::fs::read(path).with_context(|| format!("reading {}", path.display()))?,
			);
			// Same convention as the node: a hex string if it parses as one, raw bytes otherwise.
			match std::str::from_utf8(&raw).ok().and_then(decode_hex_key) {
				Some(hex) => Zeroizing::new(hex),
				None => raw,
			}
		},
		_ => bail!("pass exactly one of --node-key or --node-key-file"),
	};
	<[u8; 32]>::try_from(bytes.as_slice())
		.map(Zeroizing::new)
		.map_err(|_| anyhow!("the node key must be 32 bytes, got {}", bytes.len()))
}

/// Parse a multiaddr and check it the way the runtime will.
fn parse_addr(addr: &str) -> Result<Multiaddr> {
	let addr: Multiaddr = addr.parse().with_context(|| format!("`{addr}` is not a multiaddr"))?;
	addr_check::validate(&addr.to_vec()).map_err(|e| match e {
		addr_check::Error::ContainsPeerId => anyhow!(
			"the address must not contain a /p2p/ component; the peer id is derived from the node key"
		),
		other => anyhow!(
			"the runtime would reject `{addr}`: {other:?}; the address must be \
			 /(ip4|ip6|dns|dns4|dns6)/<host>/tcp/<port> optionally followed by /ws or /wss"
		),
	})?;
	Ok(addr)
}

enum Client {
	Ws(WsClient),
	Http(HttpClient),
}

impl Client {
	async fn connect(url: &str) -> Result<Self> {
		let client = if url.starts_with("http://") || url.starts_with("https://") {
			Self::Http(HttpClientBuilder::default().build(url)?)
		} else {
			Self::Ws(WsClientBuilder::default().build(url).await?)
		};
		Ok(client)
	}

	async fn request<R: DeserializeOwned>(&self, method: &str, params: ArrayParams) -> Result<R> {
		let response = match self {
			Self::Ws(client) => client.request(method, params).await,
			Self::Http(client) => client.request(method, params).await,
		};
		response.with_context(|| format!("RPC call `{method}` failed"))
	}
}

/// What the tool needs to know about the pallet, taken from the runtime metadata.
struct PalletMeta {
	storage_prefix: String,
	constants: BTreeMap<String, Vec<u8>>,
}

fn pallet_meta(metadata: &[u8], name: &str) -> Result<PalletMeta> {
	let prefixed = RuntimeMetadataPrefixed::decode(&mut &metadata[..])
		.context("decoding the runtime metadata")?;
	macro_rules! from_pallets {
		($metadata:expr) => {{
			let pallet = $metadata.pallets.iter().find(|p| p.name == name).ok_or_else(|| {
				anyhow!(
					"pallet `{name}` is not in the runtime metadata; pass --pallet with the name \
					 used in `construct_runtime`"
				)
			})?;
			PalletMeta {
				storage_prefix: pallet
					.storage
					.as_ref()
					.map(|s| s.prefix.clone())
					.unwrap_or_else(|| name.to_owned()),
				constants: pallet
					.constants
					.iter()
					.map(|c| (c.name.clone(), c.value.clone()))
					.collect(),
			}
		}};
	}
	Ok(match &prefixed.1 {
		RuntimeMetadata::V14(m) => from_pallets!(m),
		RuntimeMetadata::V15(m) => from_pallets!(m),
		RuntimeMetadata::V16(m) => from_pallets!(m),
		other => bail!("unsupported runtime metadata version {}", other.version()),
	})
}

/// A connection to a node, pinned to one block.
struct Chain {
	client: Client,
	at: H256,
	pallet: PalletMeta,
}

impl Chain {
	async fn connect(args: &ChainArgs) -> Result<Self> {
		let client = Client::connect(&args.url).await?;
		let at = match args.at {
			Some(at) => at,
			None => client
				.request::<Option<H256>>("chain_getBlockHash", rpc_params![])
				.await?
				.ok_or_else(|| anyhow!("the node returned no best block hash"))?,
		};
		let metadata: Bytes = client.request("state_getMetadata", rpc_params![at]).await?;
		let pallet = pallet_meta(&metadata.0, &args.pallet)?;
		Ok(Self { client, at, pallet })
	}

	fn constant<T: Decode>(&self, name: &str) -> Result<T> {
		let value = self
			.pallet
			.constants
			.get(name)
			.ok_or_else(|| anyhow!("the pallet has no `{name}` constant"))?;
		T::decode(&mut &value[..]).with_context(|| format!("decoding the `{name}` constant"))
	}

	/// Storage key of a `StorageValue`.
	fn value_key(&self, item: &str) -> Vec<u8> {
		let mut key = twox_128(self.pallet.storage_prefix.as_bytes()).to_vec();
		key.extend(twox_128(item.as_bytes()));
		key
	}

	async fn storage<T: Decode>(&self, key: Vec<u8>) -> Result<Option<T>> {
		let raw: Option<Bytes> = self
			.client
			.request("state_getStorage", rpc_params![Bytes(key), self.at])
			.await?;
		raw.map(|raw| T::decode(&mut &raw.0[..]).context("decoding a storage value"))
			.transpose()
	}

	async fn bootnodes(&self) -> Result<Vec<StoredBootnode>> {
		Ok(self.storage(self.value_key("Bootnodes")).await?.unwrap_or_default())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	const PEER: &str = "12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp";

	fn sign_cmd(node_key: Option<&str>, node_key_file: Option<PathBuf>) -> SignCmd {
		SignCmd {
			chain: ChainArgs { url: String::new(), at: None, pallet: String::new() },
			addr: String::new(),
			node_key: node_key.map(str::to_owned),
			node_key_file,
			expiry_slot: 0,
			para_id: None,
			jam_genesis_hash: None,
			offline: true,
		}
	}

	#[test]
	fn node_key_from_hex_argument() {
		let hex = "01".repeat(32);
		assert_eq!(*load_node_key(&sign_cmd(Some(&hex), None)).unwrap(), [1u8; 32]);
		assert_eq!(*load_node_key(&sign_cmd(Some(&format!("0x{hex}")), None)).unwrap(), [1u8; 32]);
		assert!(load_node_key(&sign_cmd(Some("0102"), None)).is_err());
		assert!(load_node_key(&sign_cmd(None, None)).is_err());
	}

	#[test]
	fn node_key_from_file_in_either_format() {
		let dir = std::env::temp_dir().join(format!("bootnodes-cli-{}", std::process::id()));
		std::fs::create_dir_all(&dir).unwrap();

		let raw = dir.join("secret_ed25519");
		std::fs::write(&raw, [7u8; 32]).unwrap();
		assert_eq!(*load_node_key(&sign_cmd(None, Some(raw))).unwrap(), [7u8; 32]);

		let hex = dir.join("node-key.hex");
		std::fs::write(&hex, format!("{}\n", "07".repeat(32))).unwrap();
		assert_eq!(*load_node_key(&sign_cmd(None, Some(hex))).unwrap(), [7u8; 32]);

		let short = dir.join("short");
		std::fs::write(&short, [7u8; 31]).unwrap();
		assert!(load_node_key(&sign_cmd(None, Some(short))).is_err());

		std::fs::remove_dir_all(dir).unwrap();
	}

	#[test]
	fn parse_addr_matches_runtime_rules() {
		let addr = parse_addr("/ip4/1.2.3.4/tcp/30333").unwrap();
		assert_eq!(addr, "/ip4/1.2.3.4/tcp/30333".parse::<Multiaddr>().unwrap());
		// The signed bytes are the parsed form, so that is the form to print.
		assert_eq!(
			parse_addr("/ip6/2001:0db8::0001/tcp/30333").unwrap().to_string(),
			"/ip6/2001:db8::1/tcp/30333"
		);
		let with_peer = parse_addr(&format!("/ip4/1.2.3.4/tcp/30333/p2p/{PEER}")).unwrap_err();
		assert!(with_peer.to_string().contains("/p2p/"), "{with_peer}");
		let quic = parse_addr("/ip4/1.2.3.4/udp/30333/quic-v1").unwrap_err();
		assert!(quic.to_string().contains("UnsupportedProtocol(273)"), "{quic}");
		assert!(parse_addr("not an address").is_err());
	}

	#[test]
	fn sign_output_carries_the_entry_in_genesis_form() {
		let pair = ed25519::Pair::from_seed(&[1u8; 32]);
		let genesis = H256::repeat_byte(0x4a);
		let addr = parse_addr("/ip4/1.2.3.4/tcp/30333").unwrap();
		let bootnode = sign_bootnode(&pair, &addr, 2_000, &genesis, 7);
		let out = serde_json::to_value(SignOut::new(None, 2_000, genesis, &addr, bootnode.clone()))
			.unwrap();

		// `bootnode` is what a runtime with a 128 byte address bound takes at genesis.
		let entry: Bootnode<ConstU32<128>> =
			serde_json::from_value(out["bootnode"].clone()).unwrap();
		assert_eq!(serde_json::to_value(&entry).unwrap(), out["bootnode"]);
		assert_eq!(entry.node_key, pair.public());
		assert_eq!(entry.addr.to_vec(), addr.to_vec());
		assert_eq!(entry.expiry_slot, 7);
		assert!(ed25519::Pair::verify(
			&entry.sig,
			pop_payload(&genesis, 2_000, &entry.addr, 7),
			&entry.node_key
		));

		// The human-readable fields describe the same entry.
		assert_eq!(out["node_key"], serde_json::json!(hex(pair.public().as_ref())));
		assert_eq!(out["addr"], serde_json::json!("/ip4/1.2.3.4/tcp/30333"));
		assert_eq!(out["expiry_slot"], serde_json::json!(7));
		assert_eq!(out["encoded_bootnode"], serde_json::json!(hex(&bootnode.encode())));
	}

	#[test]
	fn peer_id_matches_the_node_key() {
		// The well-known peer id of the all-ones ed25519 seed.
		let public = ed25519::Pair::from_seed(&[1u8; 32]).public();
		let peer = peer_id(&public).unwrap();
		assert_eq!(
			PeerId::from_ed25519(&<[u8; 32]>::from(public)).unwrap().to_bytes(),
			peer.to_bytes()
		);
		// A peer id derived from an ed25519 key is the identity multihash of the protobuf key.
		assert!(peer.to_string().starts_with("12D3KooW"));
	}

	#[test]
	fn storage_keys_follow_frame_layout() {
		let pallet = PalletMeta { storage_prefix: "Bootnodes".into(), constants: BTreeMap::new() };
		let chain = Chain {
			client: Client::Http(HttpClientBuilder::default().build("http://127.0.0.1:1").unwrap()),
			at: H256::zero(),
			pallet,
		};
		let mut expected = twox_128(b"Bootnodes").to_vec();
		expected.extend(twox_128(b"Bootnodes"));
		assert_eq!(chain.value_key("Bootnodes"), expected);
	}
}
