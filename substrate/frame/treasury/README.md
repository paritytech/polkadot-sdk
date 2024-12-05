# Treasury Pallet

The Treasury pallet provides a "pot" of funds that can be managed by stakeholders in the system and
a structure for making spending proposals from this pot.

## Overview

The Treasury Pallet itself provides the pot to store funds, and a means for stakeholders to propose,
approve, and deny expenditures. The chain will need to provide a method (e.g.inflation, fees) for
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

## Interface

### Dispatchable Functions

General spending/proposal protocol:
- `spend_local` - Propose and approve a spend of treasury funds, enables the
  creation of spends using the native currency of the chain, utilizing the funds
  stored in the pot
- `spend` - Propose and approve a spend of treasury funds, allows spending any
  asset kind managed by the treasury
- `remove_approval` - Force a previously approved proposal to be removed from
  the approval queue
- `payout` - Claim a spend
- `check_status` - Check the status of the spend and remove it from the storage
  if processed
- `void_spend` - Void previously approved spend
