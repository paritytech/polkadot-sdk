# Account Migration

Accounts are migrated with all their balance, locks and reserves at the beginning of the Asset Hub
migration.

## User Impact

Users need to be aware that all of their funds will be moved from the Relay chain to the Asset Hub.
The Account ID will stay the same. This ensures that normal user accounts will be to control their
funds on Asset Hub.

- 🚨 All funds will be **moved** from the Relay Chain to the Asset Hub.
- 🚨 Account IDs of parachain sovereign accounts will be translated from their Relay child to their sibling parachain account.
- The Account ID of normal accounts will stay the same.

## Sovereign Account Translation

For parachain sovereign accounts, it is not possible to just use the same account ID. The sovereign
account address of a parachain is calculated differently, depending on whether it is the account on
the Relay or a parachain (like Asset Hub).  

There are different kinds of sovereign accounts. In this context, we only focus on these parachain
sovereign accounts:
- On the Relay: derived from `"para" ++ para_id ++ 00..`
- On the Asset Hub and all other sibling parachains: derived from `"sibl" ++ para_id ++ 00..`

Our translation logic inverts the derivation and changes the prefix from `"para"` to `"sibl"` for
all accounts that match the pattern `"para" ++ para_id ++ 00..`. The full list of translated
accounts is in [this CSV file](./sovereign_account_translation.csv).

It is advised that parachains check that they can control their account on Asset Hub. They can also
forego this check if they do not need control thereof - for example when they are not holding any
funds on their relay sovereign account. However, please note that someone could still send funds to
that address before or after the migration.

Example for Bifrost: this is the [relay sovereign account](https://polkadot.subscan.io/account/13YMK2eeopZtUNpeHnJ1Ws2HqMQG6Ts9PGCZYGyFbSYoZfcm) and it gets translated to this [sibling sovereign account](https://assethub-polkadot.subscan.io/account/13cKp89TtYknbyYnqnF6dWN75q5ZosvFSuqzoEVkUAaNR47A).

## XCM

The migration happens over XCM. There will be events emitted for the balance being removed from the
Relay Chain and events emitted for the balance being deposited into Asset Hub.

### Provider and Consumer References

After inspecting the state, it’s clear that fully correcting all reference counts is nearly
impossible. Some accounts have over `10` provider references, which are difficult to trace and
reason about. To unwind all of them properly, we would need to analyze the codebase and state
history, which is not feasible.

Before an account is fully withdrawn from the Relay Chain (RC), we will force-update its consumer
and provider references to ensure it can be completely removed. If an account is intended to remain
(fully or partially) on RC, we will update the references accordingly.

To ensure the correct provider and consumer reference counts are established on the Asset Hub (AH),
we inspect the migrating pallets and reallocate the references on AH based on their logic. The
existential deposit (ED) provider reference and hold/freeze consumer references will be
automatically restored, since we use the fungible implementation to reallocate holds/freezes, rather
than manipulating state directly.

Below is a list of known sources of provider and consumer references, with notes on how they are
handled.

Pallets Increasing Provider References (Polkadot / Kusama / Westend):

- delegate_staking (P/K/W): One extra provider reference should be migrated to AH for every account
with the hold reason `pallet_delegated_staking::HoldReason::StakingDelegation`. This ensures the
entire balance, including the ED, can be staked via holds.
Source: https://github.com/paritytech/polkadot-sdk/blob/ab1e12ab6f6c3946c3c61b97328702e719cd1223/substrate/frame/delegated-staking/src/types.rs#L81

- parachains_on_demand (P/K/W): The on-demand pallet pot account should not be migrated to AH and
will remain untouched.
Source: https://github.com/paritytech/polkadot-sdk/blob/ace62f120fbc9ec617d6bab0a5180f0be4441537/polkadot/runtime/parachains/src/on_demand/mod.rs#L407

- crowdloan (P/K/W): The provider reference for a crowdloan fund account allows it to exist without
an ED until funding is received. Since new crowdloans can no longer be created, and only successful
ones are being migrated, we don’t expect any new fund accounts below ED. This reference can be
ignored.
Source: https://github.com/paritytech/polkadot-sdk/blob/9abe25d974f6045d1e97537e0f1e860459053722/polkadot/runtime/common/src/crowdloan/mod.rs#L417

- balances (P/K/W): No special handling is needed, as this is covered by the fungible implementation
during injection on AH.
Source: https://github.com/paritytech/polkadot-sdk/blob/9abe25d974f6045d1e97537e0f1e860459053722/substrate/frame/balances/src/lib.rs#L1035

- session (P/K/W): Validator accounts may receive a provider reference at genesis if they did not
previously exist. This is not relevant for migration. Even if a validator is fully reaped during
migration, they can restore their account by teleporting funds to RC post-migration.
Source: https://github.com/paritytech/polkadot-sdk/blob/8d4138f77106a6af49920ad84f3283f696f3f905/substrate/frame/session/src/lib.rs#L462-L465

- broker (//_): Not relevant for RC and AH runtimes.

Pallets Increasing Consumer References (Polkadot / Kusama / Westend):

- balances (P/K/W): No custom handling is required, as this is covered by the fungible
implementation during account injection on AH.
Source: https://github.com/paritytech/polkadot-sdk/blob/9abe25d974f6045d1e97537e0f1e860459053722/substrate/frame/balances/src/lib.rs#L1035

- recovery (/K/W): A consumer reference is added to the proxy account when it claims an already
initiated recovery process. This reference is later removed when the recovery process ends. For
simplicity, we can ignore this consumer reference, as it might affect only a small number of
accounts, and a decrease without a prior increase will not cause any issues.
See test: `polkadot_integration_tests_ahm::tests::test_account_references`
Source: https://github.com/paritytech/polkadot-sdk/blob/ace62f120fbc9ec617d6bab0a5180f0be4441537/substrate/frame/recovery/src/lib.rs#L610

- session (P/K/W): Validator accounts may be removed from RC during migration (unless they maintain
HRMP channels or register a parachain). Validators who later wish to interact with the session
pallet (e.g., set/remove keys) will need to teleport funds to RC and reinitialize their account. The
only possible inconsistency is if a validator removes already existing keys, causing the consumer
count to decrement from 0 (if no holds/freezes) or from 1 otherwise. Case 1: From 0 — no issue.
Case 2: From 1 — results in a temporarily incorrect consumer count, which will self-correct on any
account update.
See test: `polkadot_integration_tests_ahm::tests::test_account_references`
Source: https://github.com/paritytech/polkadot-sdk/blob/ace62f120fbc9ec617d6bab0a5180f0be4441537/substrate/frame/session/src/lib.rs#L812

- staking (P/K/W): No references are migrated in the new staking pallet version; legacy references are not relevant. TODO: confirm with @Ank4n

- assets, contracts, nfts, uniques, revive (//): Not relevant for RC and AH runtimes.
