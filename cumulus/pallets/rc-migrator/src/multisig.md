## Pallet Multisig

### User Impact

- 🚨 Multisigs are **not migrated** to the Asset Hub. They need to be re-announced by the user on AH.
- Multisig deposits will be migrated and unlocked on AH.

### Context

The issue with the `multisig` pallet is that every Multisig is scoped to a specific call hash. It is
not possible to just create a Multisig between Alice and Bob - it must always be scoped to a
specific call hash. A Multisig is only valid for its specific call hash.

Now, migrating call hashes from the relay to AH is dangerous. The preimage data of that hash either
does not decode anymore (best case) or decodes to something else (worse case). We can therefore  not
migrate the pure state of the `multisig` pallet. The only thing that goes amiss are previous
approvals on a specific call hash by the Multisig members.

One thing to consider is that Multisigs are constructed from account IDs. In order to allow the same
Multisigs to be re-created, it is paramount to keep all account IDs that were accessible on the
relay still accessible, hence: https://github.com/polkadot-fellows/runtimes/issues/526. Otherwise it
could happen that a Multisig cannot be re-created and loses funds to its associated accounts.

Note: I considered an XCM where the call is sent back to the relay to execute instead of executing
on AH. This would allow to migrate Multisigs, but we either need to create a new pallet for this or
change the existing one. Both probably not worth it for us now.

### Actionable

The only thing that we should do is to unlock the deposits on the AH since they were migrated to AH
with the account state.
