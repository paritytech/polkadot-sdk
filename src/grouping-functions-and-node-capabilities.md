# Grouping functions and node capabilities

All JSON-RPC functions in this interface are distributed between so-called groups.

The group a JSON-RPC function belongs to is indicated by the `prefix_` in its name. This prefix includes a version number. For example, in `foo_v1_bar` the prefix is `foo_v1`.

## Node capabilities

This JSON-RPC interface is intended to be implemented on various types of nodes:

- Full nodes, which hold in their database all block headers and bodies, plus the storage of recent blocks.
- Light clients, which only know the headers of recent blocks.
- Archive nodes, which hold in their database all block headers, bodies, and storage of every past block.
- Plain databases that aren't actually connected to the peer-to-peer network of the blockchain.

These various node implementations have different capabilities, and it is normal for some implementations to only support some functions and not others. It doesn't make sense for example for a light client to support a function that rotates keys.

JSON-RPC server must always support the `rpc_methods` function, and clients should use this function to determine which other functions are supported.

In order to not introducing too much confusion and complexity, supporting a group of functions is all or nothing. Either all functions in a group are supported, or none of them. This rule reduces the complexity that a JSON-RPC client has to face to determine what a JSON-RPC server is capable of.

## Upgradability

A group name must always include a version number. This version number is part of the group name it self, and consequently `foo_v1` and `foo_v2` should be considered as two completely separate, unrelated, groups.

As explained in the previous section, some nodes might support some groups and not others. For example, some nodes might support only `foo_v1`, some others only `foo_v2`, some others both, and some others neither.

Each group must be self-contained, and not build on top of functions of a different group. For example, `foo_v1_start` can only be stopped with `foo_v1_stop` and `foo_v2_start` can only be stopped with `foo_v2_stop`. `foo_v1_stop` must not be able to stop `foo_v2_start` and vice versa. For this reason, if the version number of a group is increased, all functions that are still relevant should be duplicated in the new group.

Having a clear version number makes it clear for developers that some functions (with a higher version number) are preferred over some others. For example, when `foo_v2` is introduced, developers automatically understand that `foo_v1` is soft-deprecated.

## Unstable functions

No guarantee is offered as to the stability of functions with `unstable` as their version number. They can disappear, get renamed, change parameters, or change return value without any warning.

For obvious reasons, higher-level applications should not rely on unstable functions. Please open an issue in this repository if some critical feature is missing or is only covered by an unstable function.

The presence of unstable functions in the interface exposed by a JSON-RPC server is not a problem by itself. Not all functions are destined to be stabilized, and it is normal for some functions to remain unstable forever.

Unstable functions are especially useful for core/parachain developers to add debugging utilities to their client implementation. For example, if a developer wants to investigate the list of Grandpa votes, they could introduce a function named `grandpa_unstable_votes`.

It is expected that all new functions added to this interface go through a testing phase during which they're unstable.
