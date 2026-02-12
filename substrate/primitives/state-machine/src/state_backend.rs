use crate::{
	backend::{Backend, StorageIterator},
	stats::StateMachineStats,
	trie_backend::{DefaultCache, TrieBackend, TrieBackendBuilder},
	trie_backend_essence::TrieBackendStorage,
	BackendTransaction, IterArgs, NomtStorageProof, StorageKey, StorageProof, StorageValue,
	TrieCacheProvider, UsageInfo,
};

use crate::backend::{AsTrieBackend, NomtBackendTransaction};
use alloc::vec::Vec;
use codec::Codec;
use hash_db::Hasher;
use nomt::{
	hasher::Blake3Hasher, KeyReadWrite, KeyValueIterator, Nomt, Overlay as NomtOverlay, Session,
	SessionParams, WitnessMode,
};
use nomt_core::witness::Witness as NomtWitness;
use parking_lot::{ArcRwLockReadGuard, Mutex, RawRwLock, RwLock};
use sp_core::storage::{ChildInfo, StateVersion};
use sp_trie::recorder::Recorder;
use std::{
	cell::RefCell,
	collections::{BTreeMap, HashMap},
	sync::Arc,
};
use trie_db::RecordedForKey;

pub enum StateBackendBuilder<S: TrieBackendStorage<H>, H: Hasher, C = DefaultCache<H>> {
	Trie {
		storage: S,
		root: H::Out,
		recorder: Option<Recorder<H>>,
		cache: Option<C>,
	},
	Nomt {
		db: ArcRwLockReadGuard<parking_lot::RawRwLock, Nomt<Blake3Hasher>>,
		overlay: Option<Vec<Arc<NomtOverlay>>>,
	},
}

impl<S, H> StateBackendBuilder<S, H>
where
	S: TrieBackendStorage<H>,
	H: Hasher,
	H::Out: Codec,
{
	/// Create a [`TrieBackend::Trie`] state backend builder.
	pub fn new_trie(storage: S, root: H::Out) -> Self {
		Self::Trie { storage, root, recorder: None, cache: None }
	}

	/// Create a [`TrieBackend::Nomt`] state backend builder.
	pub fn new_nomt(db: ArcRwLockReadGuard<parking_lot::RawRwLock, Nomt<Blake3Hasher>>) -> Self {
		Self::Nomt { db, overlay: None }
	}
}

impl<S, H, C> StateBackendBuilder<S, H, C>
where
	S: TrieBackendStorage<H>,
	H: Hasher,
	H::Out: Codec,
	C: TrieCacheProvider<H> + Send + Sync,
{
	/// Create a new StateBackend from another one, adding a ProofRecorder.
	pub fn new_with_recorder(backend: &StateBackend<S, H, C>) -> StateBackend<&S, H, &C> {
		match &backend.inner {
			InnerStateBackend::Trie(trie_backend) => {
				let recorder_backend = TrieBackendBuilder::wrap(trie_backend)
					.with_recorder(Default::default())
					.build();
				StateBackend { inner: InnerStateBackend::Trie(recorder_backend) }
			},
			InnerStateBackend::Nomt { db, maybe_overlays, .. } => {
				let nomt_rwlock = ArcRwLockReadGuard::rwlock(db);
				let recorder = ProofRecorder::<H>::new(BackendType::Nomt);

				let nomt_backend = StateBackend::<&S, H, &C>::new_nomt_backend(
					RwLock::read_arc(nomt_rwlock),
					maybe_overlays.clone(),
				);
				nomt_backend.inject_nomt_recorder(recorder.as_nomt_recorder());
				nomt_backend
			},
		}
	}

	/// Create a state backend builder.
	pub fn new_trie_with_cache(storage: S, root: H::Out, cache: C) -> Self {
		Self::Trie { storage, root, recorder: None, cache: Some(cache) }
	}

	/// Use the given optional `recorder` for the to be configured [`TrieBackend::Trie`].
	pub fn with_trie_optional_recorder(mut self, new_recorder: Option<Recorder<H>>) -> Self {
		if let StateBackendBuilder::Trie { recorder, .. } = &mut self {
			*recorder = new_recorder;
		}
		self
	}

	/// Toggle [`TrieBackend::Nomt`] recorder.
	pub fn with_nomt_overlay(mut self, nomt_overlay: Vec<Arc<NomtOverlay>>) -> Self {
		if let StateBackendBuilder::Nomt { overlay, .. } = &mut self {
			*overlay = Some(nomt_overlay);
		}
		self
	}

	/// Use the given optional `cache` for the to be configured [`TrieBackend::Trie`].
	pub fn with_trie_optional_cache<LC>(
		mut self,
		cache: Option<LC>,
	) -> StateBackendBuilder<S, H, LC> {
		match self {
			StateBackendBuilder::Trie { storage, root, recorder, .. } =>
				StateBackendBuilder::Trie { storage, root, recorder, cache },
			_ => unreachable!(),
		}
	}

	/// Use the given `cache` for the to be configured [`TrieBackend::Trie`].
	pub fn with_trie_cache<LC>(mut self, cache: LC) -> StateBackendBuilder<S, H, LC> {
		match self {
			StateBackendBuilder::Trie { storage, root, recorder, .. } =>
				StateBackendBuilder::Trie { storage, root, recorder, cache: Some(cache) },
			_ => unreachable!(),
		}
	}

	pub fn build(self) -> StateBackend<S, H, C> {
		match self {
			StateBackendBuilder::Trie { storage, root, recorder, cache } => {
				let trie_backend = TrieBackendBuilder::<S, H>::new(storage, root)
					.with_optional_cache(cache)
					.with_optional_recorder(recorder)
					.build();
				StateBackend::new_trie_backend(trie_backend)
			},
			StateBackendBuilder::Nomt { db, overlay } =>
				StateBackend::new_nomt_backend(db, overlay),
		}
	}
}

enum InnerStateBackend<S: TrieBackendStorage<H>, H: Hasher, C> {
	Trie(TrieBackend<S, H, C>),
	Nomt {
		read_recorder: RefCell<Option<NomtReadRecorder>>,
		session: RefCell<Option<Session<Blake3Hasher>>>,
		child_deltas: RefCell<Vec<(Vec<u8>, Option<Vec<u8>>)>>,
		maybe_overlays: Option<Vec<Arc<NomtOverlay>>>,
		// NOTE: This needs to be placed after the session so the drop order
		// unlock properly the read-locks.
		db: ArcRwLockReadGuard<parking_lot::RawRwLock, Nomt<Blake3Hasher>>,
	},
}

pub struct StateBackend<S: TrieBackendStorage<H>, H: Hasher, C = DefaultCache<H>> {
	inner: InnerStateBackend<S, H, C>,
}

impl<S: TrieBackendStorage<H>, H: Hasher, C> core::fmt::Debug for StateBackend<S, H, C> {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		match &self.inner {
			InnerStateBackend::Trie(_) => write!(f, "TrieBackend"),
			InnerStateBackend::Nomt { .. } => write!(f, "NomtBackend"),
		}
	}
}

impl<S, H, C> StateBackend<S, H, C>
where
	S: TrieBackendStorage<H>,
	H: Hasher,
	H::Out: Codec,
	C: TrieCacheProvider<H> + Send + Sync,
{
	fn new_trie_backend(trie_backend: TrieBackend<S, H, C>) -> Self {
		Self { inner: InnerStateBackend::Trie(trie_backend) }
	}

	fn new_nomt_backend(
		db: ArcRwLockReadGuard<parking_lot::RawRwLock, Nomt<Blake3Hasher>>,
		maybe_overlays: Option<Vec<Arc<NomtOverlay>>>,
	) -> Self {
		// NOTE: initially every nomt session has no witness mode attached,
		// once the recorder is injected the session is dropped and a fresh
		// one with enabled witnesses is created.

		let overlays = maybe_overlays.clone().unwrap_or(vec![]);
		let params = SessionParams::default()
			.witness_mode(WitnessMode::disabled())
			.overlay(overlays.iter().map(|o| o.as_ref()))
			.unwrap();
		let session = RefCell::new(Some(db.begin_session(params)));

		Self {
			inner: InnerStateBackend::Nomt {
				read_recorder: RefCell::new(None),
				session,
				child_deltas: RefCell::new(vec![]),
				maybe_overlays,
				db,
			},
		}
	}

	/// Inject a `NomtReadRecorder` which will be used to keep track of
	/// reads and, if injected, enables the Witness creation.
	pub fn inject_nomt_recorder(&self, new_recorder: NomtReadRecorder) {
		match &self.inner {
			InnerStateBackend::Nomt { session, read_recorder, maybe_overlays, db, .. } => {
				// PANIC: A recorder cannot be inserted twice.
				assert!(read_recorder.borrow().is_none());

				let overlays = maybe_overlays.clone().unwrap_or(vec![]);
				let params = SessionParams::default()
					.witness_mode(WitnessMode::read_write())
					.overlay(overlays.iter().map(|o| o.as_ref()))
					.unwrap();
				let new_session = Some(db.begin_session(params));
				*session.borrow_mut() = new_session;
				*read_recorder.borrow_mut() = Some(new_recorder);
			},
			_ => unreachable!(),
		}
	}

	pub fn get_recorder(self) -> Option<ProofRecorder<H>> {
		match self.inner {
			InnerStateBackend::Trie(trie_backend) =>
				trie_backend.extract_recorder().map(ProofRecorder::Trie),
			InnerStateBackend::Nomt { read_recorder, .. } =>
				read_recorder.into_inner().map(ProofRecorder::Nomt),
		}
	}

	fn trie(&self) -> Option<&TrieBackend<S, H, C>> {
		match &self.inner {
			InnerStateBackend::Trie(trie_backend) => Some(trie_backend),
			InnerStateBackend::Nomt { .. } => None,
		}
	}
}

impl<S, H, C> StateBackend<S, H, C>
where
	S: TrieBackendStorage<H>,
	H: Hasher,
	H::Out: Codec,
	C: TrieCacheProvider<H> + Send + Sync,
{
	pub fn root(&self) -> &H::Out {
		match &self.inner {
			InnerStateBackend::Trie(trie_backend) => trie_backend.root(),
			InnerStateBackend::Nomt { .. } => unreachable!(),
		}
	}
}

fn child_trie_key(child_info: &ChildInfo, key: &[u8]) -> Vec<u8> {
	let prefix = child_info.prefixed_storage_key();
	let mut full_key = Vec::with_capacity(prefix.len() + key.len());
	full_key.extend(prefix.clone().into_inner());
	full_key.extend(key);
	full_key
}

impl<S, H, C> crate::backend::Backend<H> for StateBackend<S, H, C>
where
	S: TrieBackendStorage<H>,
	H: Hasher,
	H::Out: Codec + Ord,
	C: TrieCacheProvider<H> + Send + Sync,
{
	type Error = crate::DefaultError;
	type TrieBackendStorage = S;
	type RawIter = RawIter<S, H, C>;

	fn storage(&self, key: &[u8]) -> Result<Option<StorageValue>, Self::Error> {
		match &self.inner {
			InnerStateBackend::Trie(trie_backend) => trie_backend.storage(key),
			InnerStateBackend::Nomt { session, read_recorder, .. } => {
				// TODO: if read recorder is enabled there could be a `warm_up` read to
				// reduce the i/o required during the commit.
				// The same applies for writes, that otherwise are entirely traversed
				// while committing the db.

				let val = session
					.borrow()
					.as_ref()
					.ok_or("Session must be open".to_string())?
					.read(key.to_vec())
					.map_err(|e| format!("{e:?}"))?;

				// TODO: lock acquisition within the critical path is not the best
				// thing to do. This should be refactored with a lock free data structure
				// such as an imbl map.
				if let Some(NomtReadRecorder { ref staging_reads, .. }) =
					*read_recorder.borrow_mut()
				{
					staging_reads.lock().insert(key.to_vec(), val.clone());
				}
				Ok(val)
			},
		}
	}

	fn storage_hash(&self, key: &[u8]) -> Result<Option<H::Out>, Self::Error> {
		match &self.inner {
			InnerStateBackend::Trie(trie_backend) => trie_backend.storage_hash(key),
			InnerStateBackend::Nomt { session, .. } => Ok(session
				.borrow()
				.as_ref()
				.ok_or("Session must be open".to_string())?
				.read_hash(key.to_vec())
				.map_err(|e| format!("{e:?}"))?
				.map(|hash: [u8; 32]| sp_core::hash::convert_hash(&hash))),
		}
	}

	fn child_storage(
		&self,
		child_info: &ChildInfo,
		key: &[u8],
	) -> Result<Option<Vec<u8>>, Self::Error> {
		match &self.inner {
			InnerStateBackend::Trie(trie_backend) => trie_backend.child_storage(child_info, key),
			InnerStateBackend::Nomt { .. } => self.storage(&child_trie_key(child_info, key)),
		}
	}

	fn child_storage_hash(
		&self,
		child_info: &ChildInfo,
		key: &[u8],
	) -> Result<Option<H::Out>, Self::Error> {
		match &self.inner {
			InnerStateBackend::Trie(trie_backend) =>
				trie_backend.child_storage_hash(child_info, key),
			InnerStateBackend::Nomt { .. } => self.storage_hash(&child_trie_key(child_info, key)),
		}
	}

	fn closest_merkle_value(
		&self,
		key: &[u8],
	) -> Result<Option<sp_trie::MerkleValue<H::Out>>, Self::Error> {
		match &self.inner {
			InnerStateBackend::Trie(trie_backend) => trie_backend.closest_merkle_value(key),
			InnerStateBackend::Nomt { .. } => todo!(),
		}
	}

	fn child_closest_merkle_value(
		&self,
		child_info: &ChildInfo,
		key: &[u8],
	) -> Result<Option<sp_trie::MerkleValue<H::Out>>, Self::Error> {
		match &self.inner {
			InnerStateBackend::Trie(trie_backend) =>
				trie_backend.child_closest_merkle_value(child_info, key),
			InnerStateBackend::Nomt { .. } => todo!(),
		}
	}

	fn exists_storage(&self, key: &[u8]) -> Result<bool, Self::Error> {
		match &self.inner {
			InnerStateBackend::Trie(trie_backend) => trie_backend.exists_storage(key),
			InnerStateBackend::Nomt { session, .. } => {
				let exists = session
					.borrow()
					.as_ref()
					.ok_or("Session must be open".to_string())?
					.read_hash(key.to_vec())
					.map_err(|e| format!("{e:?}"))?
					.is_some();
				Ok(exists)
			},
		}
	}

	fn exists_child_storage(
		&self,
		child_info: &ChildInfo,
		key: &[u8],
	) -> Result<bool, Self::Error> {
		match &self.inner {
			InnerStateBackend::Trie(trie_backend) =>
				trie_backend.exists_child_storage(child_info, key),
			InnerStateBackend::Nomt { .. } => self.exists_storage(&child_trie_key(child_info, key)),
		}
	}

	fn next_storage_key(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
		match &self.inner {
			InnerStateBackend::Trie(trie_backend) => trie_backend.next_storage_key(key),
			InnerStateBackend::Nomt { session, .. } => {
				let mut iter = session.borrow_mut().as_mut().unwrap().iterator(key.to_vec(), None);
				Ok(iter.next().map(|(key, _val)| key))
			},
		}
	}

	fn next_child_storage_key(
		&self,
		child_info: &ChildInfo,
		key: &[u8],
	) -> Result<Option<Vec<u8>>, Self::Error> {
		match &self.inner {
			InnerStateBackend::Trie(trie_backend) =>
				trie_backend.next_child_storage_key(child_info, key),
			InnerStateBackend::Nomt { .. } =>
				self.next_storage_key(&child_trie_key(child_info, key)),
		}
	}

	fn storage_root<'a>(
		&self,
		delta: impl Iterator<Item = (&'a [u8], Option<&'a [u8]>)>,
		state_version: StateVersion,
	) -> (H::Out, BackendTransaction<H>) {
		// NOTE: used to benchmark how much time did it take to calculate the storage root
		// and copute the required db changes.
		// let init_time = std::time::Instant::now();
		let res = match &self.inner {
			InnerStateBackend::Trie(trie_backend) =>
				trie_backend.storage_root(delta, state_version),
			InnerStateBackend::Nomt { child_deltas, session, read_recorder, .. } => {
				let child_deltas = std::mem::take(&mut *child_deltas.borrow_mut()).into_iter().map(
					|(key, maybe_val)| {
						(
							key.to_vec(),
							KeyReadWrite::Write(
								maybe_val.as_ref().map(|inner_val| inner_val.to_vec()),
							),
						)
					},
				);

				// Joing reads that happened within this session before performing the update.
				let read_recorder = read_recorder.borrow_mut();
				let mut reads = read_recorder.as_ref().map(|read_recorder| {
					read_recorder.join_staging_reads();
					// TODO: Reads currently need to be moved within the Proof, which
					// requires cloning them here. It would be better to avoid this cloning.
					read_recorder.reads.lock().clone()
				});

				// TODO: `if let` refactor.
				let mut actual_access: Vec<_> = if reads.is_none() {
					delta
						.into_iter()
						.map(|(key, maybe_val)| {
							let key = key.to_vec();
							// NOTE: use in case of rollbacks enabled.
							// session.borrow().as_ref().unwrap().preserve_prior_value(key.clone());
							(
								key,
								KeyReadWrite::Write(
									maybe_val.as_ref().map(|inner_val| inner_val.to_vec()),
								),
							)
						})
						.chain(child_deltas)
						.collect()
				} else {
					// UNWRAP: Above reads have been checked to not be None.
					// let mut reads = reads.unwrap().lock();
					let mut reads = reads.unwrap();
					let mut actual_access = vec![];
					for (key, maybe_val) in delta.into_iter() {
						let maybe_val = maybe_val.as_ref().map(|inner_val| inner_val.to_vec());
						let key = key.to_vec();
						// NOTE: use in case of rollbacks enabled.
						// session.borrow().as_ref().unwrap().preserve_prior_value(key.clone());
						let key_read_write = match reads.remove(&key) {
							Some(prev_val) => KeyReadWrite::ReadThenWrite(prev_val, maybe_val),
							None => KeyReadWrite::Write(maybe_val),
						};
						actual_access.push((key, key_read_write));
					}
					actual_access
						.extend(reads.into_iter().map(|(key, val)| (key, KeyReadWrite::Read(val))));
					actual_access.extend(child_deltas);
					actual_access
				};

				actual_access.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));

				// NOTE: Used for debugging
				// {
				// 	use std::io::Write;
				// 	let serialization = serde_json::to_string(&actual_access).unwrap();
				// 	let mut n = 0;
				// 	let mut path_name = format!("actual{}", n);
				// 	while std::fs::exists(&path_name).unwrap() {
				// 		n += 1;
				// 		path_name = format!("actual{}", n);
				// 	}
				// 	let mut output = std::fs::File::create(path_name).unwrap();
				//     write!(output, "{}", serialization);
				// }

				// UNWRAP: Session is expected to be open.
				let mut finished = std::mem::take(&mut *session.borrow_mut())
					.unwrap()
					.finish(actual_access)
					.unwrap();
				let witness = finished.take_witness();

				if witness.is_some() {
					if let Some(read_recorder) = read_recorder.as_ref() {
						*read_recorder.witness.lock() = witness;
					}
				}

				let root = finished.root().into_inner();
				let overlay = finished.into_overlay();

				(
					sp_core::hash::convert_hash(&root),
					BackendTransaction::new_nomt_transaction(NomtBackendTransaction {
						transaction: overlay,
					}),
				)
			},
		};

		//log::info!("storage root took: {}ms", init_time.elapsed().as_millis());
		res
	}

	fn child_storage_root<'a>(
		&self,
		child_info: &ChildInfo,
		delta: impl Iterator<Item = (&'a [u8], Option<&'a [u8]>)>,
		state_version: StateVersion,
	) -> Option<(H::Out, bool, BackendTransaction<H>)> {
		match &self.inner {
			InnerStateBackend::Trie(trie_backend) =>
				trie_backend.child_storage_root(child_info, delta, state_version),
			InnerStateBackend::Nomt { child_deltas, .. } => {
				let prefix = child_info.prefixed_storage_key();
				let child_trie_delta: Vec<_> = delta
					.map(|(k, maybe_val)| {
						let mut full_key = Vec::with_capacity(prefix.len() + k.len());
						full_key.extend(prefix.clone().into_inner());
						full_key.extend(k);
						(full_key, maybe_val.map(|val| val.to_vec()))
					})
					.collect();
				child_deltas.borrow_mut().extend(child_trie_delta);
				None
			},
		}
	}

	fn raw_iter(&self, args: IterArgs) -> Result<Self::RawIter, Self::Error> {
		match &self.inner {
			InnerStateBackend::Trie(trie_backend) =>
				trie_backend.raw_iter(args).map(|iter| Self::RawIter::new_trie_iterator(iter)),
			InnerStateBackend::Nomt { session, .. } => {
				Ok(Self::RawIter::new_nomt_iterator(
					// UNWRAP: Session is expected to be open.
					&mut *session.borrow_mut().as_mut().unwrap(),
					args,
				))
			},
		}
	}

	fn register_overlay_stats(&self, stats: &StateMachineStats) {
		match &self.inner {
			InnerStateBackend::Trie(trie_backend) => trie_backend.register_overlay_stats(stats),
			InnerStateBackend::Nomt { .. } => todo!(),
		}
	}

	fn usage_info(&self) -> UsageInfo {
		match &self.inner {
			InnerStateBackend::Trie(trie_backend) => trie_backend.usage_info(),
			InnerStateBackend::Nomt { .. } => todo!(),
		}
	}
}

impl<S: TrieBackendStorage<H>, H: Hasher, C> AsTrieBackend<H, C> for StateBackend<S, H, C>
where
	C: TrieCacheProvider<H> + Send + Sync,
	H::Out: Codec,
{
	type TrieBackendStorage = S;

	fn as_trie_backend(&self) -> &TrieBackend<S, H, C> {
		// NOTE: panic for now
		self.trie().unwrap()
	}
}

impl<S: TrieBackendStorage<H>, H: Hasher> crate::backend::AsStateBackend<H> for StateBackend<S, H> {
	type TrieBackendStorage = S;

	fn as_state_backend(&self) -> &StateBackend<S, H> {
		&self
	}
}

enum InnerRawIter<S, H, C>
where
	H: Hasher,
	H::Out: Codec,
{
	Trie(crate::trie_backend_essence::RawIter<S, H, C, Recorder<H>>),
	Nomt(RefCell<std::iter::Peekable<KeyValueIterator>>),
}

pub struct RawIter<S, H, C>
where
	H: Hasher,
	H::Out: Codec,
{
	inner: InnerRawIter<S, H, C>,
}

impl<S, H, C> RawIter<S, H, C>
where
	H: Hasher,
	H::Out: Codec,
{
	pub fn new_trie_iterator(
		iter: crate::trie_backend_essence::RawIter<S, H, C, Recorder<H>>,
	) -> Self {
		Self { inner: InnerRawIter::Trie(iter) }
	}

	pub fn new_nomt_iterator(nomt_session: &mut Session<Blake3Hasher>, args: IterArgs) -> Self {
		let start = match (&args.prefix, &args.start_at) {
			(Some(prefix), None) => prefix.to_vec(),
			(None, Some(start_at)) => start_at.to_vec(),
			(Some(prefix), Some(start_at)) => {
				assert!(start_at.starts_with(prefix));
				start_at.to_vec()
			},
			(None, None) => vec![0],
		};

		let end = if let Some(prefix) = &args.prefix {
			let mut end = prefix.to_vec();
			for byte in end.iter_mut().rev() {
				*byte = byte.wrapping_add(1);
				if *byte != 0 {
					break;
				}
			}
			Some(end)
		} else {
			None
		};

		let nomt_iter = RefCell::new(nomt_session.iterator(start.clone(), end).peekable());

		{
			let mut nomt_iter_mut = nomt_iter.borrow_mut();
			match nomt_iter_mut.peek().map(|(key, val)| key) {
				Some(first_key) if args.start_at_exclusive && *first_key == start => {
					let _ = nomt_iter_mut.next();
				},
				_ => (),
			}
		}

		Self { inner: InnerRawIter::Nomt(nomt_iter) }
	}
}

impl<S, H, C> Default for RawIter<S, H, C>
where
	H: Hasher,
	H::Out: Codec,
{
	fn default() -> Self {
		todo!("")
	}
}

impl<S, H, C> StorageIterator<H> for RawIter<S, H, C>
where
	H: Hasher,
	H::Out: Codec + Ord,
	S: TrieBackendStorage<H>,
	C: TrieCacheProvider<H> + Send + Sync,
{
	type Backend = StateBackend<S, H, C>;
	type Error = crate::DefaultError;

	fn next_key(
		&mut self,
		backend: &Self::Backend,
	) -> Option<core::result::Result<StorageKey, crate::DefaultError>> {
		match &mut self.inner {
			InnerRawIter::Trie(trie_iter) => trie_iter.next_key(backend.trie().unwrap()),
			InnerRawIter::Nomt(nomt_iter) =>
				nomt_iter.borrow_mut().next().map(|(key, _val)| Ok(key)),
		}
	}

	fn next_pair(
		&mut self,
		backend: &Self::Backend,
	) -> Option<core::result::Result<(StorageKey, StorageValue), crate::DefaultError>> {
		match &mut self.inner {
			InnerRawIter::Trie(trie_iter) => trie_iter.next_pair(backend.trie().unwrap()),
			InnerRawIter::Nomt(nomt_iter) => nomt_iter.borrow_mut().next().map(|pair| Ok(pair)),
		}
	}

	fn was_complete(&self) -> bool {
		match &self.inner {
			InnerRawIter::Trie(trie_iter) => trie_iter.was_complete(),
			InnerRawIter::Nomt(nomt_iter) => nomt_iter.borrow_mut().peek().is_some(),
		}
	}
}

// NOTE: This struct behaves differently for trie or nomt backend.
// `ProofRecorder` is not even a proper name for what this struct does
// on top of the nomt backend.
//
// ProofRecorder::Trie effectively records the visited nodes while
// ProofRecorder::Nomt instead just records (stores) the reads that happen on the
// state. This is used to specify that a witness needs to be created if
// an update happens and to carry reads around reads performed on multiple states,
// usually one for each extrinsic execution.
#[derive(Clone)]
pub enum ProofRecorder<H: Hasher> {
	Trie(sp_trie::recorder::Recorder<H>),
	Nomt(NomtReadRecorder),
}

#[derive(Clone)]
pub struct NomtReadRecorder {
	reads: Arc<Mutex<BTreeMap<Vec<u8>, Option<Vec<u8>>>>>,
	staging_reads: Arc<Mutex<BTreeMap<Vec<u8>, Option<Vec<u8>>>>>,
	witness: Arc<Mutex<Option<NomtWitness>>>,
}

impl NomtReadRecorder {
	fn join_staging_reads(&self) {
		let staging_reads: BTreeMap<Vec<u8>, Option<Vec<u8>>> =
			std::mem::take(&mut self.staging_reads.lock());
		self.reads.lock().extend(staging_reads.into_iter());
	}

	fn drain_storage_proof(self) -> NomtStorageProof {
		// UNWRAP: the recorder is moved here, it is not expected to be used anymore
		// within other state backends.
		let witness = Arc::into_inner(self.witness).unwrap().into_inner().unwrap();
		let reads = Arc::into_inner(self.reads)
			.unwrap()
			.into_inner()
			.into_iter()
			.filter_map(|(_key, maybe_val)| maybe_val)
			.collect();
		NomtStorageProof { witness, reads }
	}
}

#[derive(PartialEq, Eq)]
pub enum BackendType {
	Trie,
	Nomt,
}

impl<H: Hasher> ProofRecorder<H> {
	pub fn new(backend_type: BackendType) -> Self {
		match backend_type {
			BackendType::Trie => ProofRecorder::Trie(Default::default()),
			BackendType::Nomt => ProofRecorder::Nomt(NomtReadRecorder {
				reads: Arc::new(Mutex::new(Default::default())),
				staging_reads: Arc::new(Mutex::new(Default::default())),
				witness: Arc::new(Mutex::new(None)),
			}),
		}
	}

	pub fn with_ignored_nodes(_ignored_nodes: sp_trie::recorder::IgnoredNodes<H::Out>) -> Self {
		todo!()
		// ignored_nodes.assert_empty();
		// ProofRecorder { inner: std::cell::OnceCell::new() }
	}

	pub fn as_trie_recorder(&self) -> sp_trie::recorder::Recorder<H> {
		match self {
			ProofRecorder::Trie(trie_recorder) => trie_recorder.clone(),
			_ => unreachable!(),
		}
	}

	pub fn as_nomt_recorder(&self) -> NomtReadRecorder {
		match self {
			ProofRecorder::Nomt(nomt_read_recorder) => nomt_read_recorder.clone(),
			_ => unreachable!(),
		}
	}

	pub fn drain_storage_proof(self) -> StorageProof {
		// NOTE: The external recorder needs to be able to drain the proof from the backend.
		match self {
			ProofRecorder::Trie(trie_recorder) =>
				StorageProof::Trie(trie_recorder.drain_storage_proof()),
			ProofRecorder::Nomt(nomt_recorder) =>
				StorageProof::Nomt(nomt_recorder.drain_storage_proof()),
		}
	}

	pub fn to_storage_proof(&self) -> StorageProof {
		todo!()
	}

	pub fn estimate_encoded_size(&self) -> usize {
		todo!()
	}

	pub fn reset(&self) {
		todo!()
	}

	pub fn recorded_keys(&self) -> HashMap<H::Out, HashMap<Arc<[u8]>, RecordedForKey>> {
		todo!()
	}

	pub fn start_transaction(&self) {
		match self {
			ProofRecorder::Trie(recorder) => recorder.start_transaction(),
			ProofRecorder::Nomt(nomt_read_recorder) => {
				// PANIC: A new transaction cannot start if the previous one
				// has not been committed or rolled back.
				assert!(nomt_read_recorder.staging_reads.lock().is_empty());
			},
		}
	}

	pub fn rollback_transaction(&self) -> Result<(), ()> {
		match self {
			ProofRecorder::Trie(recorder) => recorder.rollback_transaction(),
			ProofRecorder::Nomt(nomt_read_recorder) => {
				// Delete all staging reads
				*nomt_read_recorder.staging_reads.lock() = BTreeMap::new();
				Ok(())
			},
		}
	}

	pub fn commit_transaction(&self) -> Result<(), ()> {
		match self {
			ProofRecorder::Trie(recorder) => recorder.commit_transaction(),
			ProofRecorder::Nomt(nomt_read_recorder) => {
				nomt_read_recorder.join_staging_reads();
				Ok(())
			},
		}
	}
}

impl<H: Hasher> sp_trie::ProofSizeProvider for ProofRecorder<H> {
	fn estimate_encoded_size(&self) -> usize {
		todo!()
	}
}
