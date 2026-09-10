# Treasury Pallet

The Treasury pallet provides a "pot" of funds that can be managed by stakeholders in the system and
a structure for making spending proposals from this pot.

## Overview

The Treasury Pallet itself provides the pot to store funds, and a means for stakeholders to propose
and approve expenditures. The chain will need to provide a method (e.g. inflation, fees) for
collecting funds.

By way of example, the Council could vote to fund the Treasury with a portion of the block reward
and use the funds to pay developers.

### Terminology

- **Proposal:** A suggestion to allocate funds from the pot to a beneficiary.
- **Beneficiary:** An account who will receive the funds from a proposal if the proposal is
  approved.
- **Deposit:** Funds that a proposer must lock when making a proposal. The deposit will be returned
  or slashed if the proposal is approved or rejected respectively.
- **Pot:** Unspent funds accumulated by the treasury pallet.
- **Spend:** An approved payment tracked by the pallet's `Spends` storage and claimed via
  `payout`.

## Interface

### Dispatchable Functions

General spending protocol:

- `spend` - Propose and approve a spend of treasury funds for any asset kind managed by the
  treasury.
- `payout` - Claim an approved spend.
- `check_status` - Check the status of a spend and remove it from storage if processed.
- `void_spend` - Void a previously approved spend.

### Legacy proposals

The deprecated `spend_local` and `remove_approval` dispatchables have been removed. Any legacy
`Proposals` / `Approvals` storage left on a chain is drained by
`migration::migrate_legacy_proposals::Migration` at upgrade time.

If the pot cannot cover an approved legacy proposal at upgrade, the migration defers it and logs a
warning. **Before enacting the upgrade, fund the pot, pay the proposal manually, or remove the
approval.** Once the migration is removed from the runtime's `Migrations` tuple, deferred entries
are orphaned with no code path left to pay them out.
