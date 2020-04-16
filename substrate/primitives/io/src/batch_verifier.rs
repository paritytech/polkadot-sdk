// Copyright 2020 Parity Technologies (UK) Ltd.
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

//! Batch/parallel verification.

use sp_core::{ed25519, sr25519, crypto::Pair, traits::CloneableSpawn};
use std::sync::{Arc, atomic::{AtomicBool, Ordering as AtomicOrdering}};
use futures::{future::FutureExt, task::FutureObj, channel::oneshot};

#[derive(Debug, Clone)]
struct Sr25519BatchItem {
	signature: sr25519::Signature,
	pub_key: sr25519::Public,
	message: Vec<u8>,
}

/// Batch verifier.
///
/// Used to parallel-verify signatures for runtime host. Provide task executor and
/// just push (`push_ed25519`, `push_sr25519`) as many signature as you need. At the end,
/// call `verify_and_clear to get a result. After that, batch verifier is ready for the
/// next batching job.
pub struct BatchVerifier {
	scheduler: Box<dyn CloneableSpawn>,
	sr25519_items: Vec<Sr25519BatchItem>,
	invalid: Arc<AtomicBool>,
	pending_tasks: Vec<oneshot::Receiver<()>>,
}

impl BatchVerifier {
	pub fn new(scheduler: Box<dyn CloneableSpawn>) -> Self {
		BatchVerifier {
			scheduler,
			sr25519_items: Default::default(),
			invalid: Arc::new(false.into()),
			pending_tasks: vec![],
		}
	}

	fn spawn_verification_task(
		&mut self, f: impl FnOnce() -> bool + Send + 'static,
	) -> Result<(), ()> {
		// there is already invalid transaction encountered
		if self.invalid.load(AtomicOrdering::Relaxed) { return Err(()); }

		let invalid_clone = self.invalid.clone();
		let (sender, receiver) = oneshot::channel();
		self.pending_tasks.push(receiver);

		self.scheduler.spawn_obj(FutureObj::new(async move {
			if !f() {
				invalid_clone.store(true, AtomicOrdering::Relaxed);
			}
			if sender.send(()).is_err() {
				// sanity
				log::warn!("Verification halted while result was pending");
				invalid_clone.store(true, AtomicOrdering::Relaxed);
			}
		}.boxed())).map_err(drop)
	}

	/// Push ed25519 signature to verify.
	///
	/// Returns false if some of the pushed signatures before already failed the check
	/// (in this case it won't verify anything else)
	pub fn push_ed25519(
		&mut self,
		signature: ed25519::Signature,
		pub_key: ed25519::Public,
		message: Vec<u8>,
	) -> bool {
		if self.invalid.load(AtomicOrdering::Relaxed) { return false; }

		if self.spawn_verification_task(move || ed25519::Pair::verify(&signature, &message, &pub_key)).is_err() {
			log::debug!(
				target: "runtime",
				"Batch-verification returns false because failed to spawn background task.",
			);

			return false;
		}
		true
	}

	/// Push sr25519 signature to verify.
	///
	/// Returns false if some of the pushed signatures before already failed the check.
	/// (in this case it won't verify anything else)
	pub fn push_sr25519(
		&mut self,
		signature: sr25519::Signature,
		pub_key: sr25519::Public,
		message: Vec<u8>,
	) -> bool {
		if self.invalid.load(AtomicOrdering::Relaxed) { return false; }
		self.sr25519_items.push(Sr25519BatchItem { signature, pub_key, message });
		true
	}

	/// Verify all previously pushed signatures since last call and return
	/// aggregated result.
	#[must_use]
	pub fn verify_and_clear(&mut self) -> bool {
		use std::sync::{Mutex, Condvar};

		let pending = std::mem::replace(&mut self.pending_tasks, vec![]);

		log::trace!(
			target: "runtime",
			"Batch-verification: {} pending tasks, {} sr25519 signatures",
			pending.len(),
			self.sr25519_items.len(),
		);

		let messages = self.sr25519_items.iter().map(|item| &item.message[..]).collect();
		let signatures = self.sr25519_items.iter().map(|item| &item.signature).collect();
		let pub_keys = self.sr25519_items.iter().map(|item| &item.pub_key).collect();

		if !sr25519::verify_batch(messages, signatures, pub_keys) {
			self.sr25519_items.clear();

			return false;
		}

		self.sr25519_items.clear();

		if pending.len() > 0 {
			let pair = Arc::new((Mutex::new(()), Condvar::new()));
			let pair_clone = pair.clone();

			if self.scheduler.spawn_obj(FutureObj::new(async move {
				futures::future::join_all(pending).await;
				pair_clone.1.notify_all();
			}.boxed())).is_err() {
				log::debug!(
					target: "runtime",
					"Batch-verification returns false because failed to spawn background task.",
				);

				return false;
			}

			let (mtx, cond_var) = &*pair;
			let mtx = mtx.lock().expect("Locking can only fail when the mutex is poisoned; qed");
			let _ = cond_var.wait(mtx).expect("Waiting can only fail when the mutex waited on is poisoned; qed");
		}

		!self.invalid.swap(false, AtomicOrdering::Relaxed)
	}
}
