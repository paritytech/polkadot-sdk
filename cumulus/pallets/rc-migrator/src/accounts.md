# Account Migration

Accounts are migrated with all their balance, locks and reserves at the beginning of the Asset Hub
migration.

## User Impact

Users need to be aware that all of their funds will be moved from the Relay chain to the Asset Hub.
The Account ID will stay the same. This ensures that normal user accounts will be to control their
funds on Asset Hub.

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
