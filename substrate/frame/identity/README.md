# pallet-identity

## Identity Pallet

- [`Config`]
- [`Call`]

### Overview

A federated naming system, allowing for multiple registrars to be added from a specified origin.
Registrars can set a fee to provide identity-verification service. Anyone can put forth a
proposed identity for a fixed deposit and ask for review by any number of registrars (paying
each of their fees). Registrar judgements are given as an `enum`, allowing for sophisticated,
multi-tier opinions.

Some judgements are identified as *sticky*, which means they cannot be removed except by
complete removal of the identity, or by the registrar. Judgements are allowed to represent a
portion of funds that have been reserved for the registrar.

A super-user can remove accounts and in doing so, slash the deposit.

All accounts may also have a limited number of sub-accounts which may be specified by the owner;
by definition, these have equivalent ownership and each has an individual name.

The number of registrars should be limited, and the deposit made sufficiently large, to ensure
no state-bloat attack is viable.

#### Usernames

The pallet provides functionality for username authorities to issue usernames, which are independent
of the identity information functionality; an account can set:
- an identity without setting a username
- a username without setting an identity
- an identity and a username

The username functionality implemented in this pallet is meant to be a user friendly lookup of
accounts. There are mappings in both directions, "account -> username" and "username -> account".

To grant a username, a username authority can either:
- be given an allocation by governance of a specific amount of usernames to issue for free,
  without any deposit associated with storage costs;
- put up a deposit for each username it issues (usually a subsidized, reduced deposit, relative
  to other deposits in the system).

Users can have multiple usernames that map to the same `AccountId`, however one `AccountId` can only
map to a single username, known as the _primary_. This primary username will be the result of a
lookup in the `UsernameOf` map for any given account.

### Interface

#### Dispatchable Functions

##### For General Users
- `set_identity` - Set the associated identity of an account; a small deposit is reserved if not
  already taken.
- `clear_identity` - Remove an account's associated identity; the deposit is returned.
- `request_judgement` - Request a judgement from a registrar, paying a fee.
- `cancel_request` - Cancel the previous request for a judgement.
- `accept_username` - Accept a username issued by a username authority.
- `remove_expired_approval` - Remove a username that was issued but never accepted.
- `set_primary_username` - Set a given username as an account's primary.
- `remove_username` - Remove a username after its grace period has ended.

##### For General Users with Sub-Identities
- `set_subs` - Set the sub-accounts of an identity.
- `add_sub` - Add a sub-identity to an identity.
- `remove_sub` - Remove a sub-identity of an identity.
- `rename_sub` - Rename a sub-identity of an identity.
- `quit_sub` - Remove a sub-identity of an identity (called by the sub-identity).

##### For Registrars
- `set_fee` - Set the fee required to be paid for a judgement to be given by the registrar.
- `set_fields` - Set the fields that a registrar cares about in their judgements.
- `provide_judgement` - Provide a judgement to an identity.

##### For Username Authorities
- `set_username_for` - Set a username for a given account. The account must approve it.
- `unbind_username` - Start the grace period for a username.

##### For Superusers
- `add_registrar` - Add a new registrar to the system.
- `kill_identity` - Forcibly remove the associated identity; the deposit is lost.
- `add_username_authority` - Add an account with the ability to issue usernames.
- `remove_username_authority` - Remove an account with the ability to issue usernames.
- `kill_username` - Forcibly remove a username.

[`Call`]: ./enum.Call.html
[`Config`]: ./trait.Config.html

License: Apache-2.0
