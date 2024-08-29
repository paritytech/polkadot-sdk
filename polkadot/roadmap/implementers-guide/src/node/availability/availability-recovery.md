# Availability Recovery

This subsystem is responsible for recovering the data made available via the
[Availability Distribution](availability-distribution.md) subsystem, neccessary for candidate validation during the
approval/disputes processes. Additionally, it is also being used by collators to recover PoVs in adversarial scenarios
where the other collators of the para are censoring blocks.

According to the Polkadot protocol, in order to recover any given `AvailableData`, we generally must recover at least
`f + 1` pieces from validators of the session. Thus, we should connect to and query randomly chosen validators until we
have received `f + 1` pieces.

In practice, there are various optimisations implemented in this subsystem which avoid querying all chunks from
different validators and/or avoid doing the chunk reconstruction altogether.

## Protocol

This version of the availability recovery subsystem is based only on request-response network protocols.

Input:

* `AvailabilityRecoveryMessage::RecoverAvailableData(candidate, session, backing_group, core_index, response)`

Output:

* `NetworkBridgeMessage::SendRequests`
* `AvailabilityStoreMessage::QueryAllChunks`
* `AvailabilityStoreMessage::QueryAvailableData`
* `AvailabilityStoreMessage::QueryChunkSize`


## Functionality

We hold a state which tracks the currently ongoing recovery tasks. A `RecoveryTask` is a structure encapsulating all
network tasks needed in order to recover the available data in respect to a candidate.

Each `RecoveryTask` has a collection of ordered recovery strategies to try.

```rust
/// Subsystem state.
struct State {
  /// Each recovery task is implemented as its own async task,
  /// and these handles are for communicating with them.
  ongoing_recoveries: FuturesUnordered<RecoveryHandle>,
  /// A recent block hash for which state should be available.
  live_block: (BlockNumber, Hash),
  /// An LRU cache of recently recovered data.
  availability_lru: LruMap<CandidateHash, CachedRecovery>,
  /// Cached runtime info.
  runtime_info: RuntimeInfo,
}

struct RecoveryParams {
  /// Discovery ids of `validators`.
  pub validator_authority_keys: Vec<AuthorityDiscoveryId>,
  /// Number of validators.
  pub n_validators: usize,
  /// The number of regular chunks needed.
  pub threshold: usize,
  /// The number of systematic chunks needed.
  pub systematic_threshold: usize,
  /// A hash of the relevant candidate.
  pub candidate_hash: CandidateHash,
  /// The root of the erasure encoding of the candidate.
  pub erasure_root: Hash,
  /// Metrics to report.
  pub metrics: Metrics,
  /// Do not request data from availability-store. Useful for collators.
  pub bypass_availability_store: bool,
  /// The type of check to perform after available data was recovered.
  pub post_recovery_check: PostRecoveryCheck,
  /// The blake2-256 hash of the PoV.
  pub pov_hash: Hash,
  /// Protocol name for ChunkFetchingV1.
  pub req_v1_protocol_name: ProtocolName,
  /// Protocol name for ChunkFetchingV2.
  pub req_v2_protocol_name: ProtocolName,
  /// Whether or not chunk mapping is enabled.
  pub chunk_mapping_enabled: bool,
  /// Channel to the erasure task handler.
	pub erasure_task_tx: mpsc::Sender<ErasureTask>,
}

pub struct RecoveryTask<Sender: overseer::AvailabilityRecoverySenderTrait> {
	sender: Sender,
	params: RecoveryParams,
	strategies: VecDeque<Box<dyn RecoveryStrategy<Sender>>>,
	state: task::State,
}

#[async_trait::async_trait]
/// Common trait for runnable recovery strategies.
pub trait RecoveryStrategy<Sender: overseer::AvailabilityRecoverySenderTrait>: Send {
	/// Main entry point of the strategy.
	async fn run(
		mut self: Box<Self>,
		state: &mut task::State,
		sender: &mut Sender,
		common_params: &RecoveryParams,
	) -> Result<AvailableData, RecoveryError>;

	/// Return the name of the strategy for logging purposes.
	fn display_name(&self) -> &'static str;

	/// Return the strategy type for use as a metric label.
	fn strategy_type(&self) -> &'static str;
}
```

### Signal Handling

On `ActiveLeavesUpdate`, if `activated` is non-empty, set `state.live_block_hash` to the first block in `Activated`.

Ignore `BlockFinalized` signals.

On `Conclude`, shut down the subsystem.

#### `AvailabilityRecoveryMessage::RecoverAvailableData(...)`

1. Check the `availability_lru` for the candidate and return the data if present.
1. Check if there is already a recovery handle for the request. If so, add the response handle to it.
1. Otherwise, load the session info for the given session under the state of `live_block_hash`, and initiate a recovery
   task with `launch_recovery_task`. Add a recovery handle to the state and add the response channel to it.
1. If the session info is not available, return `RecoveryError::Unavailable` on the response channel.

### Recovery logic

#### `handle_recover(...) -> Result<()>`

Instantiate the appropriate `RecoveryStrategy`es, based on the subsystem configuration, params and session info.
Call `launch_recovery_task()`.

#### `launch_recovery_task(state, ctx, response_sender, recovery_strategies, params) -> Result<()>`

Create the `RecoveryTask` and launch it as a background task running `recovery_task.run()`.

#### `recovery_task.run(mut self) -> Result<AvailableData, RecoveryError>`

* Loop:
  * Pop a strategy from the queue. If none are left, return `RecoveryError::Unavailable`.
  * Run the strategy.
  * If the strategy returned successfully or returned `RecoveryError::Invalid`, break the loop.

### Recovery strategies

#### `FetchFull`

This strategy tries requesting the full available data from the validators in the backing group to
which the node is already connected. They are tried one by one in a random order.
It is very performant if there's enough network bandwidth and the backing group is not overloaded.
The costly reed-solomon reconstruction is not needed.

#### `FetchSystematicChunks`

Very similar to `FetchChunks` below but requests from the validators that hold the systematic chunks, so that we avoid
reed-solomon reconstruction. Only possible if `node_features::FeatureIndex::AvailabilityChunkMapping` is enabled and
the `core_index` is supplied (currently only for recoveries triggered by approval voting).

More info in
[RFC-47](https://github.com/polkadot-fellows/RFCs/blob/main/text/0047-assignment-of-availability-chunks.md).

#### `FetchChunks`

The least performant strategy but also the most comprehensive one. It's the only one that cannot fail under the
byzantine threshold assumption, so it's always added as the last one in the `recovery_strategies` queue.

Performs parallel chunk requests to validators. When enough chunks were received, do the reconstruction.
In the worst case, all validators will be tried.

### Default recovery strategy configuration

#### For validators

If the estimated available data size is smaller than a configured constant (currently 1Mib for Polkadot or 4Mib for
other networks), try doing `FetchFull` first.
Next, if the preconditions described in `FetchSystematicChunks` above are met, try systematic recovery.
As a last resort, do `FetchChunks`.

#### For collators

Collators currently only use `FetchChunks`, as they only attempt recoveries in rare scenarios.

Moreover, the recovery task is specially configured to not attempt requesting data from the local availability-store
(because it doesn't exist) and to not reencode the data after a succcessful recovery (because it's an expensive check
that is not needed; checking the pov_hash is enough for collators).
