# People Pallet

A pallet managing the registry of proven individuals.

## Overview

The People pallet stores and manages identifiers of individuals who have proven their personhood. It
tracks their personal IDs, organizes their cryptographic keys into rings, and allows them to use
contextual aliases through authentication in extensions. When transactions include cryptographic
proofs of belonging to the people set, the pallet's transaction extension verifies these proofs
before allowing the transaction to proceed. This enables other pallets to check if actions come from
unique persons while preserving privacy through the ring-based structure.

The pallet accepts new persons after they prove their uniqueness elsewhere, stores their
information, and supports removing persons via suspensions. While other systems (e.g., wallets)
generate the proofs, this pallet handles the storage of all necessary data and verifies the proofs
when used.

## Key Features

- **Stores Identity Data**: Tracks personal IDs and cryptographic keys of proven persons
- **Organizes Keys**: Groups keys into rings to enable privacy-preserving proofs
- **Verifies Proofs**: Checks personhood proofs attached to transactions
- **Links Accounts**: Allows connecting blockchain accounts to contextual aliases
- **Manages Registry**: Adds proven persons and will support removing them

## Interface

### Dispatchable Functions

- `set_alias_account(origin, account)`: Link an account to a contextual alias. Once linked, this
  allows the account to dispatch transactions as a person with the alias origin using a regular
  signed transaction with a nonce, providing a simpler alternative to attaching full proofs.
- `unset_alias_account(origin)`: Remove an account-alias link

### Tasks

- `build_ring(origin, ring_index)`: Build or update a ring's cryptographic commitment. This task
  processes queued keys into a ring commitment that enables proof generation and verification. Since
  ring construction, or rather adding keys to the ring, is computationally expensive, it's performed
  periodically in batches rather than processing each key immediately. The batch size needs to be
  reasonably large to enhance privacy by obscuring the exact timing of when individuals' keys were
  added to the ring, making it more difficult to correlate specific persons with their keys.

### Storage Items

- `Root`: Maps ring indices to their cryptographic commitments.
- `CurrentRingIndex`: The index of the newest ring index currently being populated.
- `RingBuildingPeopleLimit`: Hint for the maximum number of people that can be included in a ring
  through a single root building call.
- `RingKeys`: Maps ring indices to the keys in each ring, tracking both already included keys and
  those waiting to be included. For each ring, it maintains the total set of keys and a counter
  indicating how many of those keys have been processed into the ring commitment.
- `RingKeysStatus`: Meta information for each ring, the number of keys and how many are actually
  included in the root.
- `PendingSuspensions`: Information about all rings which have pending key suspensions.
- `ActiveMembers`: The count of all members currently included in rings.
- `Keys`: The current individuals with their keys, active and inactive.
- `KeyMigrationQueue`: The people who enqueued their keys to be migrated, along with the new keys.
- `People`: Maps PersonalIds to their record information.
- `AliasToAccount`: Maps contextual aliases to accounts.
- `AccountToAlias`: Maps accounts to their contextual aliases.
- `AccountToPersonalId`: Maps accounts to their personal identities.
- `Chunks`: The static chunks used in ring proof verification.
- `NextPersonalId`: The personal ID to be assigned to the next person that is recognized.
- `RingsState`: The overarching state of all rings within the pallet.
- `ReservedPersonalId`: Keeps track of personal IDs that have been reserved.
- `QueuePageIndices`: The head and tail coordinates of the onboarding queue pages.
- `OnboardingQueue`: Paginated collection of people public keys ready to be included in a ring.

### Transaction Extension

The pallet provides the `AsPerson` transaction extension that allows transactions to be dispatched
with special origins: `PersonalIdentity` and `PersonalAlias`. These origins prove the transaction
comes from a unique person, either through their identity or through a contextual alias. To make use
of the personhood system, other pallets should check for these origins.

The extension verifies the proof of personhood during transaction validation and, if valid,
transforms the transaction's origin into one of these special origins.

## Usage

Other pallets can verify personhood through origin checks:

- `EnsurePersonalIdentity`: Verifies the origin represents a specific person using their PersonalId
- `EnsurePersonalAlias`: Verifies the origin has a valid alias for any context
- `EnsurePersonalAliasInContext`: Verifies the origin has a valid alias for a specific context
- `EnsureRevisedPersonalAlias`: Verifies the origin has a valid alias for any context and includes
  the revision of the member's ring
- `EnsureRevisedPersonalAliasInContext`: Verifies the origin has a valid alias for a specific
  context and includes the revision of the member's ring

License: Apache-2.0
