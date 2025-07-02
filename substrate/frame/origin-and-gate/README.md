# Origin "AND Gate" Pallet

"AND Gate" Substrate pallet that implements a mechanism for `EnsureOrigin` that requires two independent origins to approve a dispatch before it executes.

## Overview

The pallet provides a stateful mechanism for tracking proposal approvals from multiple origins across different blocks. Inspired by the multisig pallet pattern, it is adapted specifically for origin types rather than signatories.

The primary use case is to enforce that a dispatch has been approved by two different origin types (for example, requiring both governance council approval and technical committee approval).

This opens the possibility for on-chain collectives to approve proposals individually and asynchronously in governance workflows using rank-weighted voting with different approval weights that correspond to each of the different rank-based voting ranks that have been defined. The on-chain collectives do not need to coordinate to sign the same transaction, as they can each individually submit their own approval.

## Key Features

- **Stateful origin approval tracking**: Store proposals and track approvals across multiple blocks
- **Timepoint-based uniqueness**: Prevent duplicate proposals using block numbers and extrinsic indices
- **Automatic timeout/expiration**: Clean up storage for proposals that are no longer active
- **EnsureOrigin trait implementation**: Integrates with existing Runtime origin checks
- **Origin entity extraction**: Extract entities from different origin types for comparison

## Usage

### Pallet Configuration

To use the Origin "AND Gate" pallet in your runtime, include it in your `Cargo.toml` and implement its configuration trait:

```rust
parameter_types! {
    pub const MaxApprovals: u32 = 10;
    pub const ProposalLifetime: BlockNumber = 100; // 100 blocks
}

impl pallet_origin_and_gate::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type MaxApprovals = MaxApprovals;
    type Hashing = BlakeTwo256;
    type OriginId = u32; // Or change to specific type for your use case
    type ProposalLifetime = ProposalLifetime;
    type WeightInfo = pallet_origin_and_gate::weights::SubstrateWeight<Runtime>;
}
```

### Using the `AndGate` EnsureOrigin

In your runtime, you can use the `AndGate` struct to require approvals from two origins:

```rust
// Define origin that requires approval from both council and technical committees
pub type CouncilAndTechCommitteeApproval = pallet_origin_and_gate::AndGate<
    EnsureSignedBy<CouncilMembershipOrigin, AccountId>,
    EnsureSignedBy<TechnicalCommitteeMembershipOrigin, AccountId>
>;

// Use in dispatchable calls
#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::weight(T::WeightInfo::update_params())]
    pub fn update_sensitive_parameter(
        origin: OriginFor<T>,
        new_value: u32,
    ) -> DispatchResultWithPostInfo {
        // Only passes if both council and technical committees approved
        CouncilAndTechCommitteeApproval::ensure_origin(origin)?;

        // Apply parameter update logic

        Ok(().into())
    }
}
```

### Workflow

1. Member of Origin A submits proposal using `propose` call
2. Proposal is stored with unique call hash and timepoint
3. Member of Origin B approves proposal using `approve` call
4. `Call` is executed automatically if both required origins have approved
5. Clean-up possible if proposal expires before all required origins approve

## Examples

### Submit proposal from council origin

```rust
// Example code for submitting proposal
let call = Box::new(frame_system::Call::remark { remark: b"Hello, JAM!".to_vec() });
OriginAndGate::propose(council_origin, call, COUNCIL_ORIGIN_ID, None)?;
```

### Approve proposal from technical committee origin

```rust
// Example code for approving proposal
OriginAndGate::approve(technical_committee_origin, call_hash, COUNCIL_ORIGIN_ID, TECH_COMMITTEE_ORIGIN_ID)?;
```

## License

Apache 2.0
