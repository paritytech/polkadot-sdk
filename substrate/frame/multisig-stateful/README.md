# Multisig Stateful Pallet

A module to facilitate **stateful** multisig accounts. The statefulness of this means that we store a multisig account id in the state with  
related info (owners, threshold,..etc). The module affords enhanced control over administrative operations such as adding/removing owners, changing the threshold, account deletion, canceling an existing proposal. Each owner can approve/revoke a proposal.  

We use `proposal` in this module to refer to an extrinsic that is to be dispatched from a multisig account after getting enough approvals.

## Use Cases

* Corporate Governance:
In a corporate setting, multisig accounts can be employed for decision-making processes. For example, a company may require the approval of multiple executives to initiate   significant financial transactions.

* Joint Accounts:
Multisig accounts can be used for joint accounts where multiple individuals need to authorize transactions. This is particularly useful in family finances or shared  
business accounts.

* Decentralized Autonomous Organizations (DAOs):
DAOs can utilize multisig accounts to ensure that decisions are made collectively. Multiple key holders can be required to approve changes to the organization's rules or  
the allocation of funds.

... and much more.

## Stateless Multisig vs Stateful Multisig

### Overview

All of the mentioned use cases -and more- are better served by a stateful multisig account. This is because a stateful multisig account is stored in the state and allows for more control over the account itself. For example, a stateful multisig account can be deleted, owners can be added/removed, threshold can be changed, proposals can be canceled,..etc.  

A stateless multisig account is a multisig account that is not stored in the state. It is a simple call that is dispatched from a single account. This is useful for simple use cases where a multisig account is needed for a single purpose and no further control is needed over the account itself.

### Extrensics (Frame/Multisig vs Stateful Multisig) -- Skip if not familiar with Frame/Multisig

Main distinction in proposal approvals and execution between this implementation and the frame/multisig one is that this module  
has an extrinsic for each step of the process instead of having one entry point that can accept a `CallOrHash`:  

1. Start Proposal
2. Approve (called N times based on the threshold needed)
3. Execute Proposal

This is illustrated in the sequence diagram later in the README.

### Technical Comparison

Although a stateful multisig account might seem more expensive than a stateless one because it is stored in the state while stateless multisig is not, We see (on paper) that the stateless footprint is actually larger than the stateful one on the blockchain as for each extrinsic call in a stateless multisig, the caller needs to send all the owners and other parameters which are all stored on the blockchain itself.

TODO: Add benchmark results for both stateless and stateful multisig. (main thing to measure is the storage of extrinsics cost) over one year with 1K multisig accounts,
each with 5-100 users and doing 50 proposals per day.

## Sequence Diagrams

This is a sequence diagram for a multisig account with 5 owners and a threshold of 3. The multisig account is created with Alice, Bob, Charlie, Dave and Eve as initial owners. The multisig account is used to dispatch a call, which is approved by Bob and Charlie reaching the threshold of 3 including Alice as she started the proposal. The call is then executed by Dave (any owner of the multisig can execute the call even if they haven't approved once the proposal exceeds the required threshold).

       ┌─┐             ┌─┐              ┌─┐               ┌─┐            ┌─┐                                                    
       ║"│             ║"│              ║"│               ║"│            ║"│                                                    
       └┬┘             └┬┘              └┬┘               └┬┘            └┬┘                                                    
       ┌┼┐             ┌┼┐              ┌┼┐               ┌┼┐            ┌┼┐                                                    
        │               │                │                 │              │            ┌────────┐                               
       ┌┴┐             ┌┴┐              ┌┴┐               ┌┴┐            ┌┴┐           │Multisig│                               
      Alice            Bob            Charlie            Dave            Eve           └───┬────┘                               
        │               │             Create Multisig Account             │                │  ╔═════════════════════════════╗   
        │─────────────────────────────────────────────────────────────────────────────────>│  ║Owners: Alice, Bob, Charlie ░║   
        │               │                │                │               │                │  ║Threshold: 3                 ║   
        │               │                │                │               │                │  ╚═════════════════════════════╝   
        │               │                │Start Proposal  │               │                │  ╔════════════════════════════════╗
        │─────────────────────────────────────────────────────────────────────────────────>│  ║Proposal #1                    ░║
        │               │                │                │               │                │  ║Status: Pending Approval (1/3)  ║
        │               │                │                │               │                │  ╚════════════════════════════════╝
        │               │                │      Approve Proposal #1       │                │  ╔═════════════╗                   
        │               │─────────────────────────────────────────────────────────────────>│  ║Approval    ░║                   
        │               │                │                │               │                │  ║Status: 2/3  ║                   
        │               │                │                │               │                │  ╚═════════════╝                   
        │               │                │              Approve Proposal #1                │  ╔═════════════╗                   
        │               │                │────────────────────────────────────────────────>│  ║Approval    ░║                   
        │               │                │                │               │                │  ║Status: 3/3  ║                   
        │               │                │                │               │                │  ╚═════════════╝                   
        │               │                │                │  Execute Approved Proposal #1  │  ╔═══════════════╗                 
        │               │                │                │ ───────────────────────────────>  ║dispatch call ░║                 
      Alice            Bob            Charlie            Dave            Eve           ┌───┴──╚═══════════════╝                 
       ┌─┐             ┌─┐              ┌─┐               ┌─┐            ┌─┐           │Multisig│                               
       ║"│             ║"│              ║"│               ║"│            ║"│           └────────┘                               
       └┬┘             └┬┘              └┬┘               └┬┘            └┬┘                                                    
       ┌┼┐             ┌┼┐              ┌┼┐               ┌┼┐            ┌┼┐                                                    
        │               │                │                 │              │                                                     
       ┌┴┐             ┌┴┐              ┌┴┐               ┌┴┐            ┌┴┐                                                    

## Tehcnical Overview

### Storage/State

* We use 2 storage maps to store mutlisig accounts and proposals.
* The storage have 2 main lists:  
  * owners related to a a multisig account
  * approvers in each proposal  

For optimizing we're using BoundedBTreeSet to allow for efficient lookups and removals. Especially in the case of approvers, we need to be able to remove an approver from the list when they revoke their approval. (which we do lazily when `execute_proposal` is called)

### State Transition Functions

All functions have rustdoc in the code. Here is a brief overview of the functions:

* `create_multisig` - Create a multisig account with a given threshold and initial owners.
* `start_proposal` - Start a multisig proposal.
* `approve` - Approve a multisig proposal.
* `revoke` - Revoke a multisig approval from an existing proposal.
* `execute_proposal` - Execute a multisig proposal.

Note: Next functions need to be called from the multisig account itself.

* `add_owner` - Add a new owner to a multisig account.
* `remove_owner` - Remove an owner from a multisig account.
* `set_threshold` - Change the threshold of a multisig account.
* `cancel_proposal` - Cancel a multisig proposal.
* `delete_multisig` - Delete a multisig account.

### Considerations

* For cases where a multisig account is deleted, we make sure that all proposals are deleted as well.
* For cases when a multisig has pending proposals and an owner is removed, we make sure that all pending proposals are canceled (lazily when `execute_proposal` is called).
* Changing thresholds during a pending proposal is allowed without issues. We make sure that the proposal is executed with the threshold the latest threshold set.

### Development Testing

To test while developing, without a full build (thus reduce time to results):

```sh
cargo t -p pallet-multisig-stateful
```

## Future Work

* [ ] Reserve funds for all operations and refund them when it's finished.  
* [ ] Implement call filters. This will allow multisig accounts to only accept certain calls.
* [ ] Batch proposals. The ability to batch multiple calls into one proposal.  
* [ ] Add expiry to proposals. After a certain time, proposals will not accept any more approvals or executions and will be deleted.  
* [ ] Add extra identifier other than call_hash to proposals. This will allow same call to be proposed multiple times and be in pending state.  

License: Apache-2.0
