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

- `set_alias_account(origin, account)`: Link an account to a contextual alias Once linked, this
   allows the account to dispatch transactions as a person with the alias origin using a regular
   signed transaction with a nonce, providing a simpler alternative to attaching full proofs.
 - `unset_alias_account(origin)`: Remove an account-alias link.
 - `merge_rings`: Merge the people in two rings into a single, new ring.
 - `force_recognize_personhood`: Recognize a set of people without any additional checks.
 - `set_personal_id_account`: Set a personal id account.
 - `unset_personal_id_account`: Unset the personal id account.
 - `migrate_included_key`: Migrate the key for a person who was onboarded and is currently included
   in a ring.
 - `migrate_onboarding_key`: Migrate the key for a person who is currently onboarding. The operation
   is instant, replacing the old key in the onboarding queue.
 - `set_onboarding_size`: Force set the onboarding size for new people. This call requires root
   privileges.
 - `build_ring_manual`: Manually build a ring root by including registered people. The transaction
   fee is refunded on a successful call.
 - `onboard_people_manual`: Manually onboard people into a ring. The transaction fee is refunded on
   a successful call.

### Automated tasks performed by the pallet in hooks

- Ring building: Build or update a ring's cryptographic commitment. This task processes queued keys
  into a ring commitment that enables proof generation and verification. Since ring construction, or
  rather adding keys to the ring, is computationally expensive, it's performed periodically in
  batches rather than processing each key immediately. The batch size needs to be reasonably large
  to enhance privacy by obscuring the exact timing of when individuals' keys were added to the ring,
  making it more difficult to correlate specific persons with their keys.
- People onboarding: Onboard people from the onboarding queue into a ring. This task takes the
  unincluded keys of recognized people from the onboarding queue and registers them into the ring.
  People can be onboarded only in batches of at least `OnboardingSize` and when the remaining open
  slots in a ring are at least `OnboardingSize`. This does not compute the root, that is done using
  `build_ring`.
- Cleaning of suspended people: Remove people's keys marked as suspended or inactive from rings. The
  keys are stored in the `PendingSuspensions` map and they are removed from rings and their roots
  are reset. The ring roots will subsequently be build in the ring building phase from scratch.
  sequentially.
- Key migration: Migrate the keys for people who were onboarded and are currently included in rings.
  The migration is not instant as the key replacement and subsequent inclusion in a new ring root
  will happen only after the next mutation session.
- Onboarding queue page merging: Merge the two pages at the front of the onboarding queue. After a
  round of suspensions, it is possible for the second page of the onboarding queue to be left with
  few members such that, if the first page also has few members, the total count is below the
  required onboarding size, thus stalling the queue. This function fixes this by moving the people
  from the first page to the front of the second page, defragmenting the queue.

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
