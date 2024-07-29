# Runtime

Types used within the runtime exclusively and pervasively.

## Host Configuration

The internal-to-runtime configuration of the parachain host is kept in `struct HostConfiguration`. This is expected to
be altered only by governance procedures or via migrations from the Polkadot-SDK codebase. The latest definition of
`HostConfiguration` can be found in the project repo
[here](https://github.com/paritytech/polkadot-sdk/blob/master/polkadot/runtime/parachains/src/configuration.rs). Each
parameter has got a doc comment so for any details please refer to the code.

Some related parameters in `HostConfiguration` are grouped together so that they can be managed easily. These are:
* `async_backing_params` in `struct AsyncBackingParams`
* `executor_params` in `struct ExecutorParams`
* `approval_voting_params` in `struct ApprovalVotingParams`
* `scheduler_params` in `struct SchedulerParams`

Check the definitions of these structs for further details.

### Configuration migrations
Modifying `HostConfiguration` requires a storage migration. These migrations are located in the
[`migrations`](https://github.com/paritytech/polkadot-sdk/blob/master/polkadot/runtime/parachains/src/configuration.rs)
subfolder of Polkadot-SDK repo.

## ParaInherentData

Inherent data passed to a runtime entry-point for the advancement of parachain consensus.

This contains 4 pieces of data:
1. [`Bitfields`](availability.md#signed-availability-bitfield)
2. [`BackedCandidates`](backing.md#backed-candidate)
3. [`MultiDisputeStatementSet`](disputes.md#multidisputestatementset)
4. `Header`

```rust
struct ParaInherentData {
	bitfields: Bitfields,
	backed_candidates: BackedCandidates,
	dispute_statements: MultiDisputeStatementSet,
	parent_header: Header
}
```
