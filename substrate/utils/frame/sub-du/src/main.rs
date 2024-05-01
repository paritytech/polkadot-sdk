//! # `sub-du`
//!
//! A tool like [`du`](https://en.wikipedia.org/wiki/Du_(Unix)) that calculates storage size of pallets of a Substrate chain.
//!
//! # Note
//!
//! Currently, when calculating the size of `Map` storages, it reads the value of first iterated key
//! to get the size of the value and assumes all the keys have values of the same size. This is not
//! necessarily true, but it is a trade-off between speed and accuracy. Otherwise, calculating the
//! size of some prefixes (e.g. `System::Account`) would take quite a long time.
//!
//! # Usage
//!
//! ```sh
//! cargo run -- --progress --uri wss://rpc.polkadot.io:443 --count 1000
//! ```
//!
//! # Example Output
//!
//! ```sh
//! Scraping at block Some(0xb83ec17face39e35bdf29305a9f8f568775e357fc55745132ea9bd4d8ff6d976) of polkadot(1002000)
//! 152 K │ │─┬ System
//! 101 M │ │ │─ Account => Map(106,025,440 bytes, 1325318 keys)
//! 128 K │ │ │─ BlockHash => Map(131,104 bytes, 4097 keys)
//! 24  K │ │ │─ Events => Value(24,588 bytes)
//! 156 B │ │ │─ Digest => Value(156 bytes)
//! 32  B │ │ │─ ParentHash => Value(32 bytes)
//! 18  B │ │ │─ BlockWeight => Value(18 bytes)
//! 13  B │ │ │─ LastRuntimeUpgrade => Value(13 bytes)
//! 4   B │ │ │─ EventCount => Value(4 bytes)
//! 4   B │ │ │─ Number => Value(4 bytes)
//! 1   B │ │ │─ UpgradedToTripleRefCount => Value(1 bytes)
//! 1   B │ │ │─ UpgradedToU32RefCount => Value(1 bytes)
//! 0   B │ │ │─ AuthorizedUpgrade => Value(0 bytes)
//! 0   B │ │ │─ ExecutionPhase => Value(0 bytes)
//! 0   B │ │ │─ EventTopics => Map(0 bytes, 0 keys)
//! 0   B │ │ │─ ExtrinsicData => Map(0 bytes, 0 keys)
//! 0   B │ │ │─ AllExtrinsicsLen => Value(0 bytes)
//! 0   B │ │ │─ ExtrinsicCount => Value(0 bytes)
//! 1   K │ │─┬ Scheduler
//! 1   K │ │ │─ Agenda => Map(1,551 bytes, 1551 keys)
//! 24  B │ │ │─ Lookup => Map(24 bytes, 3 keys)
//! ```

use ansi_term::{Colour::*, Style};
use frame_metadata::{v14::StorageEntryType, RuntimeMetadata, RuntimeMetadataPrefixed};
use sc_rpc_api::{chain::ChainApiClient, state::StateApiClient};
use separator::Separatable;
use sp_core::{storage::StorageKey, twox_128};
use sp_runtime::testing::{Header, H256 as Hash};
use structopt::StructOpt;
use substrate_rpc_client::WsClient;

#[cfg(test)]
mod tests;

const KB: usize = 1024;
const MB: usize = KB * KB;
const GB: usize = MB * MB;

pub const LOG_TARGET: &str = "sub-du";

fn get_prefix(indent: usize) -> &'static str {
	match indent {
		1 => "├─┬",
		2 => "│ │─┬",
		3 => "│ │ │─",
		_ => panic!("Unexpected indent."),
	}
}

struct Size(usize);

impl std::fmt::Display for Size {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		if self.0 <= KB {
			write!(f, "{: <4}B", self.0)?;
		} else if self.0 <= MB {
			write!(f, "{: <4}K", self.0 / KB)?;
		} else if self.0 <= GB {
			write!(f, "{: <4}M", self.0 / MB)?;
		}

		Ok(())
	}
}

#[derive(Debug, Clone, Default)]
struct Pallet {
	pub name: String,
	pub size: usize,
	pub items: Vec<Storage>,
}

impl std::fmt::Display for Pallet {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let mod_style = Style::new().bold().italic().fg(Green);
		writeln!(
			f,
			"{} {} {}\n",
			mod_style.paint(format!("{}", Size(self.size))),
			get_prefix(2),
			mod_style.paint(self.name.clone())
		)?;
		for s in self.items.iter() {
			writeln!(f, "{} {} {}", Size(s.size), get_prefix(3), s)?;
		}
		Ok(())
	}
}

impl Pallet {
	fn new(name: String) -> Self {
		Self { name, ..Default::default() }
	}
}

#[derive(Debug, Copy, Clone)]
pub enum StorageItem {
	Value(usize),
	Map(usize, usize),
}

impl std::fmt::Display for StorageItem {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Value(bytes) => write!(f, "Value({} bytes)", bytes.separated_string()),
			Self::Map(bytes, count) => {
				write!(f, "Map({} bytes, {} keys)", bytes.separated_string(), count)
			},
		}
	}
}

impl Default for StorageItem {
	fn default() -> Self {
		Self::Value(0)
	}
}

#[derive(Debug, Clone, Default)]
struct Storage {
	pub name: String,
	pub size: usize,
	pub item: StorageItem,
}

impl std::fmt::Display for Storage {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let item_style = Style::new().italic();
		write!(f, "{} => {}", item_style.paint(self.name.clone()), self.item)
	}
}

impl Storage {
	fn new(name: String, item: StorageItem) -> Self {
		let size = match item {
			StorageItem::Value(s) => s,
			StorageItem::Map(s, _) => s,
		};
		Self { name, item, size }
	}
}

#[derive(Debug, StructOpt)]
#[structopt(
	name = "sub-du",
	about = "a du-like tool that prints the map of storage usage of a substrate chain"
)]
struct Opt {
	/// The block number at which the scrap should happen. Use only the hex value, no need for a
	/// `0x` prefix.
	#[structopt(long)]
	at: Option<Hash>,

	/// The node to connect to.
	#[structopt(long, default_value = "wss://rpc.polkadot.io:443")]
	uri: String,

	/// If true, intermediate values will be printed.
	#[structopt(long, short)]
	progress: bool,

	/// Count of keys to read in one page.
	#[structopt(long, default_value = "1000")]
	count: u32,
}

/// create key prefix for a module as vec bytes. Basically twox128 hash of the given values.
pub fn pallet_prefix_raw(module: &[u8], storage: &[u8]) -> Vec<u8> {
	let module_key = twox_128(module);
	let storage_key = twox_128(storage);
	let mut final_key = Vec::with_capacity(module_key.len() + storage_key.len());
	final_key.extend_from_slice(&module_key);
	final_key.extend_from_slice(&storage_key);
	final_key
}

/// Using `state_getStorageSize` RPC call times out since querying storage size of a relatively
/// large storage prefix takes a lot of time. This function is a workaround to get the size of the
/// storage by using paginated `state_getKeysPaged` RPC call.
///
/// It is a modified implementation of `state_getStorageSize` RPC call, with some major differences:
///
/// - uses paginated reading of keys.
/// - only reads once for the value size, instead of reading it for all keys.
///
/// The latter might not be entirely accurate, since not all values might have the same size (e.g.
/// when storage value contains `Option<T>` or unbounded data types). It is a trade-off between
/// speed and accuracy. Otherwise, calculating the size would take quite a long time.
async fn prefix_storage_size(
	client: &WsClient,
	at: Option<Hash>,
	prefix: StorageKey,
	count: u32,
) -> Option<(u64, usize)> {
	match client.storage(prefix.clone(), at).await {
		Ok(Some(d)) => return Some((d.0.len() as u64, 0)),
		Ok(None) => (),
		Err(e) => {
			log::error!(target: LOG_TARGET, "Error while reading storage: {:?}", e);
			return None;
		},
	}

	let mut sum = 0;
	let mut keys_count = 0;
	let mut value_size = None;

	let mut start_key = None;

	loop {
		let keys = client
			.storage_keys_paged(Some(prefix.clone()), count, start_key, at)
			.await
			.unwrap_or(vec![]);

		if keys.is_empty() {
			break;
		}

		let current_keys = keys.len();
		keys_count += current_keys;

		for key in keys.clone() {
			// don't really need to read for all, just for the first one to get the size.
			if let Some(size) = value_size {
				sum += size;
			} else if let Ok(Some(value)) = client.storage(key.clone(), at).await {
				value_size = Some(value.0.len() as u64);
				sum += value.0.len() as u64;
			}
		}
		if current_keys < count as usize {
			break;
		}

		start_key = if let Some(last) = keys.last() { Some(last.clone()) } else { break };
	}

	Some((sum, keys_count))
}

#[tokio::main]
async fn main() {
	let now = std::time::Instant::now();

	tracing_subscriber::fmt::try_init().unwrap();
	let opt = Opt::from_args();

	let rpc_client = substrate_rpc_client::ws_client(opt.uri).await.unwrap();
	let mut modules: Vec<Pallet> = vec![];

	// potentially replace head with the given hash
	let head = ChainApiClient::<(), _, Header, ()>::finalized_head(&rpc_client).await.unwrap();

	let at = opt.at.or(Some(head));

	let runtime = rpc_client.runtime_version(at).await.unwrap();

	println!("Scraping at block {:?} of {}({})", at, runtime.spec_name, runtime.spec_version,);

	let raw_metadata = rpc_client.metadata(at).await.unwrap();
	let prefixed_metadata = <RuntimeMetadataPrefixed as codec::Decode>::decode(&mut &*raw_metadata)
		.expect("Runtime Metadata failed to decode");
	let metadata = prefixed_metadata.1;

	let mut total_size = 0;
	match metadata {
		RuntimeMetadata::V14(inner) => {
			let pallets = inner.pallets;
			for pallet in pallets.into_iter() {
				let name = pallet.name;

				// skip, if this module has no storage items.
				if pallet.storage.is_none() {
					log::warn!(
						target: LOG_TARGET,
						"Pallet with name {:?} seems to have no storage items.",
						name
					);
					continue
				}

				let storage = pallet.storage.unwrap();
				let prefix = storage.prefix;
				let entries = storage.entries;
				let mut pallet_info = Pallet::new(name.clone());

				for storage_entry in entries.into_iter() {
					let storage_name = storage_entry.name;

					let ty = storage_entry.ty;
					let key_prefix = pallet_prefix_raw(prefix.as_bytes(), storage_name.as_bytes());

					// This should be faster
					let (size, pairs) = prefix_storage_size(
						&rpc_client,
						at,
						StorageKey(key_prefix.clone()),
						opt.count,
					)
					.await
					.unwrap();

					log::debug!(
						target: LOG_TARGET,
						"{:?}::{:?} => count: {}, size: {} bytes",
						name,
						storage_name,
						pairs,
						size
					);

					pallet_info.size += size as usize;
					let item = match ty {
						StorageEntryType::Plain(_) => StorageItem::Value(size as usize),
						StorageEntryType::Map { .. } => StorageItem::Map(size as usize, pairs),
					};
					pallet_info.items.push(Storage::new(storage_name, item));
				}
				pallet_info.items.sort_by_key(|x| x.size);
				pallet_info.items.reverse();

				if opt.progress {
					print!("{}", pallet_info);
				}
				total_size += pallet_info.size;
				modules.push(pallet_info);
			}

			modules.sort_by_key(|m| m.size);
			modules.reverse();

			if !opt.progress {
				modules.into_iter().for_each(|m| {
					print!("{}", m);
				})
			}
			println!("\nTotal size: {} {}", Size(total_size), runtime.spec_name,);
			let elapsed = now.elapsed();
			println!("{:?}s", elapsed.as_secs_f32());
		},

		_ => panic!("Unsupported metadata version."),
	}
}
