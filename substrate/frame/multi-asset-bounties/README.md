# Multi-Asset Bounties Pallet

The Multi-Asset Bounties pallet unifies and expands the functionality of the existing Bounties and Child Bounties pallets.

## Overview

A bounty is a reward for completing a specified task or achieving a defined objective. This pallet enables stakeholders to fund, curate, and award bounties using a chosen asset kind.

For example, OpenGov could vote to fund a bounty with 100 USDC from the Treasury and award it to a marketing team.

### Terminology

- **Bounty:** A reward for a predefined body of work upon completion.
- **Parent Bounty:** A Treasury-funded bounty that defines the total reward and may be subdivided into multiple child bounties.
- **Child Bounty:** A subtask or milestone funded by a parent bounty. It may carry its own curator, fee, and reward similar to the parent bounty.
**Curator:** An account managing the bounty and assigning a payout address receiving the reward for the completion of work.
**Curator fee:** The reserved upfront payment for a curator for work related to the bounty.
**Curator stash:** An account/location chosen by the curator that receives the curator fee when the child-/bounty is awarded.
**Curator deposit:** The payment in native asset from a candidate willing to curate a funded bounty. The deposit is returned when/if the bounty is completed.
**Bounty value:** The total amount in a given asset kind that should be paid to the Beneficiary and Curator stash if the bounty is rewarded.
**Beneficiary:** The account/location to which the total or part of the bounty is assigned to.

## Interface

### Dispatchable Functions

- `fund_bounty` -  Fund a new parent bounty with a proposed curator, iniitiating the payment from the treasury to the bounty account/location.
- `fund_child_bounty` - Fund a new child-bounty with a proposed curator, initiating the payment from the parent bounty to the child-bounty account/location.
- `propose_curator` - Propose a new curator for a child-/bounty after the previous was unassigned.
- `accept_curator` -Accept the curator role for a child-/bounty.
- `unassign_curator` - Unassign curator from a child-/bounty.
- `award_bounty` - Awards the child-/bounty to a beneficiary account/location, initiating the payout payments to both the beneficiary and the curator.
- `close_bounty` - Cancel an active child-/bounty. A payment to send all the funds to the funding source is initialized.
- `check_status` - Check and update the payment status of a child-/bounty.
- `retry_payment` - Retry the funding, refund or payout payments.