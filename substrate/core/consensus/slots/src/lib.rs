// Copyright 2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Slots functionality for Substrate.
//!
//! Some consensus algorithms have a concept of *slots*, which are intervals in
//! time during which certain events can and/or must occur.  This crate
//! provides generic functionality for slots.

#![forbid(warnings, unsafe_code, missing_docs)]

mod slots;

pub use slots::{Slots, SlotInfo};

use std::sync::{mpsc, Arc};
use std::thread;
use std::fmt::Debug;
use futures::prelude::*;
use futures::{Future, IntoFuture, future::{self, Either}};
use log::{warn, debug, info};
use runtime_primitives::generic::BlockId;
use runtime_primitives::traits::{ProvideRuntimeApi, Block, ApiRef};
use consensus_common::SyncOracle;
use inherents::{InherentData, InherentDataProviders};
use client::ChainHead;
use codec::{Encode, Decode};

/// A worker that should be invoked at every new slot.
pub trait SlotWorker<B: Block> {
	/// The type fo the future that will be returned when a new slot is
	/// triggered.
	type OnSlot: IntoFuture<Item=(), Error=consensus_common::Error>;

	/// Called when the proposer starts.
	fn on_start(
		&self,
		slot_duration: u64
	) -> Result<(), consensus_common::Error>;

	/// Called when a new slot is triggered.
	fn on_slot(
		&self,
		chain_head: B::Header,
		slot_info: SlotInfo,
	) -> Self::OnSlot;
}

/// Slot compatible inherent data.
pub trait SlotCompatible {
	/// Extract timestamp and slot from inherent data.
	fn extract_timestamp_and_slot(inherent: &InherentData) -> Result<(u64, u64), consensus_common::Error>;
}

/// Convert an inherent error to common error.
pub fn inherent_to_common_error(err: inherents::RuntimeString) -> consensus_common::Error {
	consensus_common::ErrorKind::InherentData(err.into()).into()
}

/// Start a new slot worker in a separate thread.
pub fn start_slot_worker_thread<B, C, W, SO, SC, T, OnExit>(
	slot_duration: SlotDuration<T>,
	client: Arc<C>,
	worker: Arc<W>,
	sync_oracle: SO,
	on_exit: OnExit,
	inherent_data_providers: InherentDataProviders,
) -> Result<(), consensus_common::Error> where
	B: Block + 'static,
	C: ChainHead<B> + Send + Sync + 'static,
	W: SlotWorker<B> + Send + Sync + 'static,
	SO: SyncOracle + Send + Clone + 'static,
	SC: SlotCompatible + 'static,
	OnExit: Future<Item=(), Error=()> + Send + 'static,
	T: SlotData + Send + Clone + 'static,
{
	use tokio::runtime::current_thread::Runtime;

	let (result_sender, result_recv) = mpsc::channel();

	thread::spawn(move || {
		let mut runtime = match Runtime::new() {
			Ok(r) => r,
			Err(e) => {
				warn!("Unable to start authorship: {:?}", e);
				return;
			}
		};

		let slot_worker_future = match start_slot_worker::<_, _, _, _, _, SC, _>(
			slot_duration.clone(),
			client,
			worker,
			sync_oracle,
			on_exit,
			inherent_data_providers,
		) {
			Ok(slot_worker_future) => {
				result_sender
					.send(Ok(()))
					.expect("Receive is not dropped before receiving a result; qed");
				slot_worker_future
			},
			Err(e) => {
				result_sender
					.send(Err(e))
					.expect("Receive is not dropped before receiving a result; qed");
				return;
			}
		};

		let _ = runtime.block_on(slot_worker_future);
	});

	result_recv.recv().expect("Slots start thread result sender dropped")
}

/// Start a new slot worker.
pub fn start_slot_worker<B, C, W, T, SO, SC, OnExit>(
	slot_duration: SlotDuration<T>,
	client: Arc<C>,
	worker: Arc<W>,
	sync_oracle: SO,
	on_exit: OnExit,
	inherent_data_providers: InherentDataProviders,
) -> Result<impl Future<Item=(), Error=()>, consensus_common::Error> where
	B: Block,
	C: ChainHead<B>,
	W: SlotWorker<B>,
	SO: SyncOracle + Send + Clone,
	SC: SlotCompatible,
	OnExit: Future<Item=(), Error=()>,
	T: SlotData + Clone,
{
	worker.on_start(slot_duration.slot_duration())?;

	let make_authorship = move || {
		let client = client.clone();
		let worker = worker.clone();
		let sync_oracle = sync_oracle.clone();
		let SlotDuration(slot_duration) = slot_duration.clone();
		let inherent_data_providers = inherent_data_providers.clone();

		// rather than use a timer interval, we schedule our waits ourselves
		Slots::<SC>::new(slot_duration.slot_duration(), inherent_data_providers)
			.map_err(|e| debug!(target: "slots", "Faulty timer: {:?}", e))
			.for_each(move |slot_info| {
				let client = client.clone();
				let worker = worker.clone();
				let sync_oracle = sync_oracle.clone();

				// only propose when we are not syncing.
				if sync_oracle.is_major_syncing() {
					debug!(target: "slots", "Skipping proposal slot due to sync.");
					return Either::B(future::ok(()));
				}

				let slot_num = slot_info.number;
				let chain_head = match client.best_block_header() {
					Ok(x) => x,
					Err(e) => {
						warn!(target: "slots", "Unable to author block in slot {}. \
							no best block header: {:?}", slot_num, e);
						return Either::B(future::ok(()))
					}
				};

				Either::A(
					worker.on_slot(chain_head, slot_info).into_future()
						.map_err(|e| debug!(target: "slots", "Encountered consensus error: {:?}", e))
				)
			})
	};

	let work = future::loop_fn((), move |()| {
		let authorship_task = ::std::panic::AssertUnwindSafe(make_authorship());
		authorship_task.catch_unwind().then(|res| {
			match res {
				Ok(Ok(())) => (),
				Ok(Err(())) => warn!(target: "slots", "Authorship task terminated unexpectedly. Restarting"),
				Err(e) => {
					if let Some(s) = e.downcast_ref::<&'static str>() {
						warn!(target: "slots", "Authorship task panicked at {:?}", s);
					}

					warn!(target: "slots", "Restarting authorship task");
				}
			}

			Ok(future::Loop::Continue(()))
		})
	});

	Ok(work.select(on_exit).then(|_| Ok(())))
}

/// A header which has been checked
pub enum CheckedHeader<H, S> {
	/// A header which has slot in the future. this is the full header (not stripped)
	/// and the slot in which it should be processed.
	Deferred(H, u64),
	/// A header which is fully checked, including signature. This is the pre-header
	/// accompanied by the seal components.
	///
	/// Includes the digest item that encoded the seal.
	Checked(H, S),
}

/// A type from which a slot duration can be obtained.
pub trait SlotData {
	/// Gets the slot duration.
	fn slot_duration(&self) -> u64;

	/// The static slot key
	const SLOT_KEY: &'static [u8];
}

impl SlotData for u64 {
	fn slot_duration(&self) -> u64 { *self }

	const SLOT_KEY: &'static [u8] = b"aura_slot_duration";
}

/// A slot duration. Create with `get_or_compute`.
// The internal member should stay private here.
#[derive(Clone, Copy, Debug, Encode, Decode, Hash, PartialOrd, Ord, PartialEq, Eq)]
pub struct SlotDuration<T: Clone>(T);

impl<T: Clone> SlotDuration<T> {
	/// Either fetch the slot duration from disk or compute it from the
	/// genesis state.
	///
	/// `slot_key` is marked as `'static`, as it should really be a
	/// compile-time constant.
	pub fn get_or_compute<B: Block, C, CB>(client: &C, cb: CB) -> ::client::error::Result<Self> where
		C: client::backend::AuxStore,
		C: ProvideRuntimeApi,
		CB: FnOnce(ApiRef<C::Api>, &BlockId<B>) -> ::client::error::Result<T>,
		T: SlotData + Encode + Decode + Debug,
	{
		match client.get_aux(T::SLOT_KEY)? {
			Some(v) => <T as codec::Decode>::decode(&mut &v[..])
				.map(SlotDuration)
				.ok_or_else(|| ::client::error::Error::Backend(
					format!("slot duration kept in invalid format"),
				).into()),
			None => {
				use runtime_primitives::traits::Zero;
				let genesis_slot_duration = cb(
					client.runtime_api(),
					&BlockId::number(Zero::zero()))?;

				info!(
					"Loaded block-time = {:?} seconds from genesis on first-launch",
					genesis_slot_duration
				);

				genesis_slot_duration.using_encoded(|s| {
					client.insert_aux(&[(T::SLOT_KEY, &s[..])], &[])
				})?;

				Ok(SlotDuration(genesis_slot_duration))
			}
		}
	}

	/// Returns slot data value.
	pub fn get(&self) -> T {
		self.0.clone()
	}

	/// Get the slot duration in milliseconds
	pub fn slot_duration(&self) -> u64
		where T: SlotData
	{
		self.0.slot_duration()
	}
}
