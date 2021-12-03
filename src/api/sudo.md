# Introduction

The functions with the `sudo` prefix are targeted at blockchain node operators who want to inspect the state of their blockchain node.

Contrary to functions with other prefixes, functions with the `sudo` prefix are meant to be called on a specific JSON-RPC server, and not for example on a load balancer. When implementing a load balancer in front of multiple JSON-RPC servers, functions with the `sudo` prefix should be forbidden.

# sudo_v1_addReservedPeer

**TODO**: same as `system_addReservedPeer` but properly defined

**TODO**: is this function actually necessary?

# sudo_v1_pendingExtrinsics

**Parameters**: *none*
**Return value**: an array of hexadecimal-encoded SCALE-encoded extrinsics that are in the transactions pool of the node

**TODO**: is this function actually necessary?

# sudo_v1_rotateKeys

**TODO**: same as `author_rotateKeys`

# sudo_v1_removeReservedPeer

**TODO**: same as `system_removeReservedPeer` but properly defined

**TODO**: is this function actually necessary?

# sudo_v1_version

**Parameters**: *none*
**Return value**: String containing a human-readable name and version of the implementation of the JSON-RPC server.

The return value shouldn't be parsed by a program. It is purely meant to be shown to a human.

**Note**: Example return values: "polkadot 0.9.12-ec34cf7e0-x86_64-linux-gnu", "smoldot-light 0.5.4"
