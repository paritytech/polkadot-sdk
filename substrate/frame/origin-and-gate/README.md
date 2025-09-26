# Origin "AND Gate" Pallet

## Overview

The "AND Gate" Substrate pallet implements a flexible mechanism for `EnsureOrigin` where `RequiredApprovalsCount` may be configured to require two or more independent origins to asynchronously approve a proposal that is proposing to dispatch a call so that it may execute before its expiry block, and supports cleanup of that proposal after a configured retention period.

## Key Features
### 1. Proposal Management

- **Creation**: Create proposals with specific origin IDs and optional expiry block
- **States of Proposal Lifecycle**: Track proposals through its complete lifecycle
  - **Pending**: Await sufficient approvals
  - **Executed**: Successfully executed after receiving `RequiredApprovalsCount` approvals
  - **Expired**: Reached `ProposalExpiry` expiry block without receiving `RequiredApprovalsCount` approvals
  - **Cancelled**: Explicitly cancelled proposal by the proposer includes automatic cleanup
- **Expiry Detection & Update (Automated)**: Uses `on_initialize` hook to automatically detect expired proposals and update their status

### 2. Origin Approval System

- **Multiple Origins**: Supports requiring approval from multiple different origins
- **Configurable Threshold**: Set the number of required approvals via `RequiredApprovalsCount`
- **Approval Tracking**: Track which origins have approved which proposals
- **Approval Withdrawal**: Origins can withdraw their approval before execution
- **Conditional Approvals**: Attach conditional remarks to approvals that can be amended later

### 3. Execution and Cleanup

- **Proposal Exection (Automatic)**: Proposals execute automatically when approval threshold of `RequiredApprovalsCount` is met
- **Storage Cleanup (Automatic)**: Terminal proposals are removed after configurable retention period `ProposalRetentionPeriodWhenNotCancelled` with the exception of cancelled proposals that are cleaned up immediately
- **Optimized Cleanup**: Executed proposals use execution time as base for retention period calculation
- **Cleanup Rules**:
  - **Cancelled** proposals are eligible for immediate cleanup upon cancellation
  - **Executed** proposals use their execution time as the base for retention period calculation
  - **Expired** proposals use their expiry time as the base for retention period calculation

## Configuration Parameters

- **RequiredApprovalsCount**: Maximum number of origin approvals required for a proposal to execute
- **ProposalExpiry**: How long (measured in blocks) a proposal remains valid before expiry
- **ProposalRetentionPeriodWhenNotCancelled**: How long to keep executed or expired proposals before cleanup (measured in blocks)
- **MaxProposalsToExpirePerBlock**: Maximum number of proposals that can expire in a single block (measured in blocks)

## Proposal Lifecycle and Cleanup

The pallet manages proposals through a complete lifecycle:

1. **Creation**: A proposal is created with a specific origin ID and optional expiry.
2. **Approval**: Origins can approve proposals, with execution occurring when `RequiredApprovalsCount` approvals are gathered.
3. **Terminal States**: Proposals can reach terminal states through:
   - **Execution**: When `RequiredApprovalsCount` approvals are gathered
   - **Expiry**: When the `ProposalExpiry` block is reached
   - **Cancellation**: When explicitly cancelled by the proposer

4. **Cleanup**: Terminal proposals with the exception of cancelled proposals are retained for a configurable period before being eligible for cleanup:
   - **Cancelled** proposals are automatically cleaned up upon cancellation
   - **Executed** proposals use their execution block as the base for retention period calculation
   - **Expired** proposals use their expiry block as the base for retention period calculation

This optimized cleanup approach ensures that storage is freed efficiently while maintaining proposal history for an appropriate period.

## Examples

### Submit proposal from an origin (ambassador fellowship)

```rust
// Alice creates a proposal to create a remark call that includes the first approval
// from an origin (ambassador fellowship). Assuming `RequiredApprovalsCount` is set to `2`
// then it requires a second approval prior to execution
let call = make_remark_call("test");
let expiry = Some(100);
assert_ok!(OriginAndGate::propose(
    RuntimeOrigin::signed(ALICE),
    call.clone(),
    AMBASSADOR_ORIGIN_ID,
    expiry,
    Some(true),  // include_proposer_approval (optional, defaults to false)
    None,        // remark
    None,        // auto_execute (optional, defaults to false)
));
```

### Approval of proposal by a second origin

```rust
// Approve the proposal from an origin (technical fellowship) that is different
// from the first origin (ambassador fellowship)
let call_hash = <<Test as Config>::Hashing as Hash>::hash_of(&call);
assert_ok!(OriginAndGate::approve(
    RuntimeOrigin::signed(BOB),
    call_hash,
    AMBASSADOR_ORIGIN_ID,
    TECH_ORIGIN_ID,
));
```

### Cancel a proposal

```rust
// Only the proposer can cancel their proposal
assert_ok!(OriginAndGate::cancel(
    RuntimeOrigin::signed(ALICE),
    call_hash,
    AMBASSADOR_ORIGIN_ID,
));
```

### Clean up a terminal proposal

```rust
// Clean up a proposal that has reached a terminal state (expired or executed)
// and passed the retention period
assert_ok!(OriginAndGate::clean(
    RuntimeOrigin::signed(ALICE),
    call_hash,
    AMBASSADOR_ORIGIN_ID,
));
```

## Extended Use Cases

### Conditional Approval

The pallet supports conditional approval patterns:

- **Stakeholder Governance**: Ensures proposals may be independently evaluated against pre-determined criteria and conditionally approved by multiple stakeholders (e.g. ambassadors, technical fellows, etc.) if they satisfy each of their requirements
- **Approval Remarks**: Origins can attach remarks to their approvals to document conditions or reasoning
- **Remark Amendment**: Approvers can amend their approval remarks without removing their approval
- **Transparency**: All approval remarks are stored on-chain for transparency and auditing

#### Conditional Approval Example

```rust
// Bob approves a proposal with a conditional approval remark
let conditional_remark = "Approved with condition: Treasury must have >1000 tokens".as_bytes().to_vec();
assert_ok!(OriginAndGate::add_approval(
    RuntimeOrigin::signed(BOB),
    call_hash,
    AMBASSADOR_ORIGIN_ID,
    TECH_ORIGIN_ID,
    true,  // auto-execute if threshold met
    Some(conditional_remark),
));

// Bob later may amend his approval with an updated remark
let amended_remark = "Approved: Treasury condition verified".as_bytes().to_vec();
assert_ok!(OriginAndGate::amend_remark(
    RuntimeOrigin::signed(BOB),
    call_hash,
    AMBASSADOR_ORIGIN_ID,
    TECH_ORIGIN_ID,
    amended_remark,
));
```

## License

Licensed under the [Apache License, Version 2.0](LICENSE).

## API Documentation

For detailed API documentation, see the [Rust Docs](https://docs.rs/pallet-origin-and-gate).
