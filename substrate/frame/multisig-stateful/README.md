# Multisig Stateful Pallet


A pallet to facilitate enhanced multisig accounts. The main enhancement is that we store a multisig account in the state with related info (owners, threshold,..etc). The module affords enhanced control over administrative operations such as adding/removing owners, changing the threshold, account deletion, canceling an existing proposal. Each owner can approve/revoke a proposal while still exists. The proposal is **not** intended for migrating or getting rid of existing multisig. It's to allow both options to coexist.

For the rest of the document we use the following terms:

* `proposal` to refer to an extrinsic that is to be dispatched from a multisig account after getting enough approvals.
* `Stateful Multisig` to refer to the proposed pallet.
* `Stateless Multi` to refer to the current multisig pallet in polkadot-sdk.

## Use Cases

* Corporate Governance:
In a corporate setting, multisig accounts can be employed for decision-making processes. For example, a company may require the approval of multiple executives to initiate   significant financial transactions.

* Joint Accounts:
Multisig accounts can be used for joint accounts where multiple individuals need to authorize transactions. This is particularly useful in family finances or shared  
business accounts.

* Decentralized Autonomous Organizations (DAOs):
DAOs can utilize multisig accounts to ensure that decisions are made collectively. Multiple key holders can be required to approve changes to the organization's rules or the allocation of funds.

... and much more.

## Stateless Multisig vs Stateful Multisig

### Overview

All of the mentioned use cases -and more- are better served by a stateful multisig account. This is because a stateful multisig account is stored in the state and allows for more control over the account itself. For example, a stateful multisig account can be deleted, owners can be added/removed, threshold can be changed, proposals can be canceled,..etc.  

A stateless multisig account is a multisig account that is not stored in the state. It is a simple call that is dispatched from a single account. This is useful for simple use cases where a multisig account is needed for a single purpose and no further control is needed over the account itself.

### Extrensics (Frame/Multisig vs Stateful Multisig) -- Skip if not familiar with Frame/Multisig

Main distinction in proposal approvals and execution between this implementation and the frame/multisig one is that this module  has an extrinsic for each step of the process instead of having one entry point that can accept a `CallOrHash`:  

1. Start Proposal
2. Approve (called N times based on the threshold needed)
3. Execute Proposal

This is illustrated in the sequence diagram later in the README.

Later we'll explain performance impact and how the suggested pallet is superior to existing stateless multisig and the effect on the blockchain footprint.

## Sequence Diagrams
                               
![multisig operations](https://github.com/paritytech/polkadot-sdk/assets/2677789/7f16b2ab-50c6-44d1-8cc3-680cd404385b)
Notes on above diagram:

* It's a 3 step process to execute a proposal. (Start Proposal --> Approvals --> Execute Proposal)
* `Execute` is an explicit extrinsic for a simpler API. It can be optimized to be executed automatically after getting enough approvals.
* Any user can create a multisig account and they don't need to be part of it. (Alice in the diagram)
* A proposal is any extrinsic including control extrinsics (e.g. add/remove owner, change threshold,..etc).
* Any multisig account owner can start a proposal on behalf of the multisig account. (Bob in the diagram)
* Any multisig account owener can execute proposal if it's approved by enough owners. (Dave in the diagram)       

## Tehcnical Overview
### State Transition Functions

All functions have detailed rustdoc in [PR#3300](https://github.com/paritytech/polkadot-sdk/pull/3300). Here is a brief overview of the functions:

* `create_multisig` - Create a multisig account with a given threshold and initial owners. (Needs Deposit)
* `start_proposal` - Start a multisig proposal. (Needs Deposit)
* `approve` - Approve a multisig proposal.
* `revoke` - Revoke a multisig approval from an existing proposal.
* `execute_proposal` - Execute a multisig proposal. (Releases Deposit)
* `cancel_own_proposal` - Cancel a multisig proposal started by the caller in case no other owners approved it yet. (Releases Deposit)

Note: Next functions need to be called from the multisig account itself. Deposits are reserved from the multisig account as well.

* `add_owner` - Add a new owner to a multisig account. (Needs Deposit)
* `remove_owner` - Remove an owner from a multisig account. (Releases Deposit)
* `set_threshold` - Change the threshold of a multisig account.
* `cancel_proposal` - Cancel a multisig proposal. (Releases Deposit)
* `delete_multisig` - Delete a multisig account. (Releases Deposit)

### Storage/State

* Use 2 main storage maps to store mutlisig accounts and proposals.

```rust
#[pallet::storage]
  pub type MultisigAccount<T: Config> = StorageMap<_, Twox64Concat, T::AccountId, MultisigAccountDetails<T>>;

/// The set of open multisig proposals. A proposal is uniquely identified by the multisig account and the call hash.
/// (maybe a nonce as well in the future)
#[pallet::storage]
pub type PendingProposals<T: Config> = StorageDoubleMap<
    _,
    Twox64Concat,
    T::AccountId, // Multisig Account
    Blake2_128Concat,
    T::Hash, // Call Hash
    MultisigProposal<T>,
>;
```

As for the values:

```rust
pub struct MultisigAccountDetails<T: Config> {
	/// The owners of the multisig account. This is a BoundedBTreeSet to ensure faster operations (add, remove).
	/// As well as lookups and faster set operations to ensure approvers is always a subset from owners. (e.g. in case of removal of an owner during an active proposal)
	pub owners: BoundedBTreeSet<T::AccountId, T::MaxSignatories>,
	/// The threshold of approvers required for the multisig account to be able to execute a call.
	pub threshold: u32,
	pub creator: T::AccountId,
	pub deposit: BalanceOf<T>,
}
```

```rust
pub struct MultisigProposal<T: Config> {
    /// Proposal creator.
    pub creator: T::AccountId,
    pub creation_deposit: BalanceOf<T>,
    /// The extrinsic when the multisig operation was opened.
    pub when: Timepoint<BlockNumberFor<T>>,
    /// The approvers achieved so far, including the depositor.
    /// The approvers are stored in a BoundedBTreeSet to ensure faster lookup and operations (approve, revoke).
    /// It's also bounded to ensure that the size don't go over the required limit by the Runtime.
    pub approvers: BoundedBTreeSet<T::AccountId, T::MaxSignatories>,
    /// The block number until which this multisig operation is valid. None means no expiry.
    pub expire_after: Option<BlockNumberFor<T>>,
}
```

For optimization we're using BoundedBTreeSet to allow for efficient lookups and removals. Especially in the case of approvers, we need to be able to remove an approver from the list when they revoke their approval. (which we do lazily when `execute_proposal` is called).

There's an extra storage map for the deposits of the multisig accounts per owner added. This is to ensure that we can release the deposits when the multisig removes them even if the constant deposit per owner changed in the runtime later on.

### Considerations & Edge cases

#### Removing an owner from the multisig account during an active proposal

 We need to ensure that the approvers are always a subset from owners. This is also partially why we're using BoundedBTreeSet for owners and approvers. Once execute proposal is called we ensure that the proposal is still valid and the approvers are still a subset from current owners.

#### Multisig account deletion and cleaning up existing proposals

Once the last owner of a multisig account is removed or the multisig approved the account deletion we delete the multisig accound from the state and keep the proposals until someone calls `cleanup_proposals` multiple times which iterates over a max limit per extrinsic. This is to ensure we don't have unbounded iteration over the proposals. Users are already incentivized to call `cleanup_proposals` to get their deposits back.

#### Multisig account deletion and existing deposits

We currently just delete the account without checking for deposits (Would like to hear your thoughts here). We can either

* Don't make deposits to begin with and make it a fee.
* Transfer to treasury.
* Error on deletion. (don't like this)

#### Approving a proposal after the threshold is changed

We always use latest threshold and don't store each proposal with different threshold. This allows the following:

* In case threshold is lower than the number of approvers then the proposal is still valid.  
* In case threshold is higher than the number of approvers then we catch it during execute proposal and error.
### Performance

Doing back of the envelop calculation to proof that the stateful multisig is more efficient than the stateless multisig given it's smaller footprint size on blocks.

Quick review over the extrinsics for both as it affects the block size:

Stateless Multisig:
Both `as_multi` and `approve_as_multi` has a similar parameters:

```rust
origin: OriginFor<T>,
threshold: u16,
other_signatories: Vec<T::AccountId>,
maybe_timepoint: Option<Timepoint<BlockNumberFor<T>>>,
call_hash: [u8; 32],
max_weight: Weight,
```

Stateful Multisig:
We have the following extrinsics:

```rust
pub fn start_proposal(
			origin: OriginFor<T>,
			multisig_account: T::AccountId,
			call_hash: T::Hash,
		)
```

```rust
pub fn approve(
			origin: OriginFor<T>,
			multisig_account: T::AccountId,
			call_hash: T::Hash,
		)
```

```rust
pub fn execute_proposal(
			origin: OriginFor<T>,
			multisig_account: T::AccountId,
			call: Box<<T as Config>::RuntimeCall>,
		)
```

The main takeway is that we don't need to pass the threshold and other signatories in the extrinsics. This is because we already have the threshold and signatories in the state (only once).

So now for the caclulations, given the following:

* K is the number of multisig accounts.
* N is number of owners in each multisig account.
* For each proposal we need to have 2N/3 approvals.

The table calculates if each of the K multisig accounts has one proposal and it gets approved by the 2N/3 and then executed. How much did the total Blocks and States sizes increased by the end of the day.

Note: We're not calculating the cost of proposal as both in statefull and stateless multisig they're almost the same and gets cleaned up from the state once the proposal is executed or canceled.

Stateless effect on blocksizes = 2/3*K*N^2 (as each user of the 2/3 users will need to call approve_as_multi with all the other signatories(N) in extrinsic body)

Stateful effect on blocksizes = K * N (as each user will need to call approve with the multisig account only in extrinsic body)

Stateless effect on statesizes = Nil (as the multisig account is not stored in the state)

Stateful effect on statesizes = K*N (as each multisig account (K) will be stored with all the owners (K) in the state)

| Pallet         |  Block Size   | State Size |
|----------------|:-------------:|-----------:|
| Stateless      |     2/3*K*N^2 |        Nil |
| Stateful       |          K*N  |       K*N  |

Simplified table removing K from the equation:
| Pallet         |  Block Size   | State Size |
|----------------|:-------------:|-----------:|
| Stateless      |           N^2 |        Nil |
| Stateful       |           N   |         N  |

So even though the stateful multisig has a larger state size, it's still more efficient in terms of block size and total footprint on the blockchain.

### Development Testing

To test while developing, without a full build (thus reduce time to results):

```sh
cargo t -p pallet-multisig-stateful
```

## Future Work

* [ ] Batch proposals. The ability to batch multiple calls into one proposal.  
* [ ] Batch addition/removal of owners.
* [ ] Add expiry to proposals. After a certain time, proposals will not accept any more approvals or executions and will be deleted.  
* [ ] Add extra identifier other than call_hash to proposals (e.g. nonce). This will allow same call to be proposed multiple times and be in pending state.  
* [ ] Implement call filters. This will allow multisig accounts to only accept certain calls.

License: Apache-2.0
