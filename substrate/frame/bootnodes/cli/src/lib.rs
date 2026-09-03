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
//! - `inspect` reads the pallet's constants, its bootnode set and the last accepted sequence
//!   numbers at the latest (or a given) block of a running node.
//! - `sign` derives the node key from the node's secret key, looks up the next sequence number on
//!   chain, and prints a signed bootnode entry ready to be submitted with `add_bootnode` by the
//!   pallet's admin origin.
//!
//! The pallet is located through the runtime metadata, so the tool needs nothing but an RPC
//! endpoint; `--para-id`, `--jam-genesis-hash` and `--seq` can override or, with `--offline`,
//! replace what would be read from the chain.
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
use multiaddr::{Multiaddr, Protocol};
use pallet_bootnodes::{multiaddr as addr_check, pop_payload, Bootnode};
use sc_network_types::PeerId;
use serde::{de::DeserializeOwned, Serialize};
use sp_core::{crypto::Pair as _, ed25519, Bytes, ConstU32, H256};
use sp_crypto_hashing::{blake2_128, twox_128};
use std::{collections::BTreeMap, path::PathBuf};

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
	/// Print the pallet's constants, bootnode set and last accepted sequence numbers.
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
	/// The node's public multiaddr, without a `/p2p/` component.
	#[arg(long)]
	pub addr: String,
	/// The node's ed25519 secret key as 64 hex characters (what the node gets as `--node-key`).
	#[arg(long, required_unless_present = "node_key_file", conflicts_with = "node_key_file")]
	pub node_key: Option<String>,
	/// File holding the node's ed25519 secret key, as hex or as 32 raw bytes (what the node gets
	/// as `--node-key-file`, or its `<base-path>/chains/<chain-id>/network/secret_ed25519`).
	#[arg(long)]
	pub node_key_file: Option<PathBuf>,
	/// Sequence number. Defaults to one more than the last one accepted on chain for this key,
	/// or 0 if the key was never listed.
	#[arg(long)]
	pub seq: Option<u64>,
	/// Para id to sign for. Defaults to the pallet's `ParaId` constant.
	#[arg(long)]
	pub para_id: Option<u32>,
	/// JAM genesis hash to sign for. Defaults to the pallet's `JamGenesisHash` constant.
	#[arg(long)]
	pub jam_genesis_hash: Option<H256>,
	/// Do not contact the chain. Requires `--seq`, `--para-id` and `--jam-genesis-hash`.
	#[arg(long, requires_all = ["seq", "para_id", "jam_genesis_hash"])]
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
	let last_seq = chain.all_last_seq().await?;

	let out = InspectOut {
		at: chain.at,
		pallet: cmd.chain.pallet,
		para_id: chain.constant("ParaId")?,
		jam_genesis_hash: chain.constant("JamGenesisHash")?,
		max_bootnodes: chain.constant("MaxBootnodes")?,
		max_addr_len: chain.constant("MaxAddrLen")?,
		bootnodes: bootnodes.iter().map(BootnodeOut::from).collect(),
		last_seq: last_seq
			.into_iter()
			.map(|(node_key, seq)| LastSeqOut {
				listed: bootnodes.iter().any(|b| b.node_key == node_key),
				node_key: hex(node_key.as_ref()),
				seq,
			})
			.collect(),
	};
	println!("{}", serde_json::to_string_pretty(&out)?);
	Ok(())
}

async fn sign(cmd: SignCmd) -> Result<()> {
	let pair = ed25519::Pair::from_seed(&load_node_key(&cmd)?);
	let node_key = pair.public();
	let addr = parse_addr(&cmd.addr)?;

	let (at, para_id, jam_genesis_hash, seq) = if cmd.offline {
		// `clap` enforces the presence of the three flags.
		let missing = |flag: &str| anyhow!("--offline requires {flag}");
		(
			None,
			cmd.para_id.ok_or_else(|| missing("--para-id"))?,
			cmd.jam_genesis_hash.ok_or_else(|| missing("--jam-genesis-hash"))?,
			cmd.seq.ok_or_else(|| missing("--seq"))?,
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
		let last = chain.last_seq(&node_key).await?;
		let seq = match (cmd.seq, last) {
			(Some(seq), Some(last)) if seq <= last => bail!(
				"--seq {seq} does not exceed the last sequence number {last} accepted for this node key"
			),
			(Some(seq), _) => seq,
			(None, Some(last)) => {
				last.checked_add(1).ok_or_else(|| anyhow!("sequence numbers exhausted"))?
			},
			(None, None) => 0,
		};
		(Some(chain.at), para_id, jam_genesis_hash, seq)
	};

	let payload = pop_payload(&jam_genesis_hash, para_id, seq, &addr);
	let sig = pair.sign(&payload);
	let bootnode = StoredBootnode {
		addr: addr.clone().try_into().map_err(|_| anyhow!("address too long"))?,
		node_key,
		seq,
		sig,
	};

	let out = SignOut {
		at,
		para_id,
		jam_genesis_hash,
		node_key: hex(node_key.as_ref()),
		peer_id: peer_id(&node_key).map(|p| p.to_string()),
		addr: cmd.addr,
		seq,
		sig: hex(sig.as_ref()),
		encoded_bootnode: hex(&bootnode.encode()),
	};
	println!("{}", serde_json::to_string_pretty(&out)?);
	Ok(())
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
	last_seq: Vec<LastSeqOut>,
}

#[derive(Serialize)]
struct BootnodeOut {
	node_key: String,
	peer_id: Option<String>,
	addr: String,
	/// The address with the peer id appended, as accepted by a node's `--bootnodes` flag.
	dial: Option<String>,
	seq: u64,
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
			seq: b.seq,
			sig: hex(b.sig.as_ref()),
		}
	}
}

#[derive(Serialize)]
struct LastSeqOut {
	node_key: String,
	seq: u64,
	/// Whether the key is currently in the bootnode set.
	listed: bool,
}

#[derive(Serialize)]
struct SignOut {
	/// Block the sequence number and constants were read at; absent with `--offline`.
	at: Option<H256>,
	para_id: u32,
	jam_genesis_hash: H256,
	node_key: String,
	peer_id: Option<String>,
	addr: String,
	seq: u64,
	sig: String,
	/// SCALE encoding of the `Bootnode`, the argument of `add_bootnode`.
	encoded_bootnode: String,
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
fn load_node_key(cmd: &SignCmd) -> Result<[u8; 32]> {
	let bytes = match (&cmd.node_key, &cmd.node_key_file) {
		(Some(hex), None) => {
			decode_hex_key(hex).ok_or_else(|| anyhow!("--node-key must be 64 hex characters"))?
		},
		(None, Some(path)) => {
			let raw = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
			// Same convention as the node: a hex string if it parses as one, raw bytes otherwise.
			std::str::from_utf8(&raw).ok().and_then(decode_hex_key).unwrap_or(raw)
		},
		_ => bail!("pass exactly one of --node-key or --node-key-file"),
	};
	<[u8; 32]>::try_from(bytes.as_slice())
		.map_err(|_| anyhow!("the node key must be 32 bytes, got {}", bytes.len()))
}

/// Parse a multiaddr and check it the way the runtime will.
fn parse_addr(addr: &str) -> Result<Vec<u8>> {
	let addr: Multiaddr = addr.parse().with_context(|| format!("`{addr}` is not a multiaddr"))?;
	if addr.iter().any(|p| matches!(p, Protocol::P2p(_))) {
		bail!("the address must not contain a /p2p/ component; the peer id is derived from the node key");
	}
	let bytes = addr.to_vec();
	addr_check::validate(&bytes)
		.map_err(|e| anyhow!("the runtime would reject `{addr}`: {e:?}"))?;
	Ok(bytes)
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

	/// Storage key of a `StorageValue`, also the key prefix of a map.
	fn value_key(&self, item: &str) -> Vec<u8> {
		let mut key = twox_128(self.pallet.storage_prefix.as_bytes()).to_vec();
		key.extend(twox_128(item.as_bytes()));
		key
	}

	/// Storage key of an entry of a `Blake2_128Concat` map.
	fn map_key(&self, item: &str, key: &impl Encode) -> Vec<u8> {
		let encoded = key.encode();
		let mut full = self.value_key(item);
		full.extend(blake2_128(&encoded));
		full.extend(encoded);
		full
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

	async fn last_seq(&self, node_key: &ed25519::Public) -> Result<Option<u64>> {
		self.storage(self.map_key("LastSeq", node_key)).await
	}

	/// Every `LastSeq` entry.
	async fn all_last_seq(&self) -> Result<Vec<(ed25519::Public, u64)>> {
		const PAGE: u32 = 256;
		let prefix = self.value_key("LastSeq");
		let mut entries = Vec::new();
		let mut start_key: Option<Bytes> = None;
		loop {
			let keys: Vec<Bytes> = self
				.client
				.request(
					"state_getKeysPaged",
					rpc_params![Bytes(prefix.clone()), PAGE, start_key.clone(), self.at],
				)
				.await?;
			for key in &keys {
				// prefix ++ blake2_128(node_key) ++ node_key
				let raw = key
					.0
					.get(prefix.len() + 16..)
					.ok_or_else(|| anyhow!("unexpected `LastSeq` key length"))?;
				let node_key =
					ed25519::Public::decode(&mut &raw[..]).context("decoding a `LastSeq` key")?;
				if let Some(seq) = self.storage::<u64>(key.0.clone()).await? {
					entries.push((node_key, seq));
				}
			}
			if keys.len() < PAGE as usize {
				break;
			}
			start_key = keys.last().cloned();
		}
		Ok(entries)
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
			seq: None,
			para_id: None,
			jam_genesis_hash: None,
			offline: true,
		}
	}

	#[test]
	fn node_key_from_hex_argument() {
		let hex = "01".repeat(32);
		assert_eq!(load_node_key(&sign_cmd(Some(&hex), None)).unwrap(), [1u8; 32]);
		assert_eq!(load_node_key(&sign_cmd(Some(&format!("0x{hex}")), None)).unwrap(), [1u8; 32]);
		assert!(load_node_key(&sign_cmd(Some("0102"), None)).is_err());
		assert!(load_node_key(&sign_cmd(None, None)).is_err());
	}

	#[test]
	fn node_key_from_file_in_either_format() {
		let dir = std::env::temp_dir().join(format!("bootnodes-cli-{}", std::process::id()));
		std::fs::create_dir_all(&dir).unwrap();

		let raw = dir.join("secret_ed25519");
		std::fs::write(&raw, [7u8; 32]).unwrap();
		assert_eq!(load_node_key(&sign_cmd(None, Some(raw))).unwrap(), [7u8; 32]);

		let hex = dir.join("node-key.hex");
		std::fs::write(&hex, format!("{}\n", "07".repeat(32))).unwrap();
		assert_eq!(load_node_key(&sign_cmd(None, Some(hex))).unwrap(), [7u8; 32]);

		let short = dir.join("short");
		std::fs::write(&short, [7u8; 31]).unwrap();
		assert!(load_node_key(&sign_cmd(None, Some(short))).is_err());

		std::fs::remove_dir_all(dir).unwrap();
	}

	#[test]
	fn parse_addr_matches_runtime_rules() {
		let bytes = parse_addr("/ip4/1.2.3.4/tcp/30333").unwrap();
		assert_eq!(bytes, "/ip4/1.2.3.4/tcp/30333".parse::<Multiaddr>().unwrap().to_vec());
		assert!(parse_addr(&format!("/ip4/1.2.3.4/tcp/30333/p2p/{PEER}")).is_err());
		assert!(parse_addr("not an address").is_err());
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

		let node_key = ed25519::Public::from_raw([9u8; 32]);
		let mut expected = twox_128(b"Bootnodes").to_vec();
		expected.extend(twox_128(b"LastSeq"));
		expected.extend(blake2_128(&node_key.encode()));
		expected.extend(node_key.encode());
		assert_eq!(chain.map_key("LastSeq", &node_key), expected);
	}
}
