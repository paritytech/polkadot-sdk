# Grouping functions and node capabilities

Let's examine *why* we're building a JSON-RPC interface at all:

- For end-user-facing applications to be able to read and interact with a blockchain.
- For node operators to make sure that their node is operating correctly, and perform some operations such as rotating keys.
- For core/parachain developers to manually look at the state of the blockchain and figure out what is on the chain.

These three audiences have different needs, and it doesn't make sense for example for a light client to support a function that rotates keys.

It is already the case today that light clients only support a subset of all JSON-RPC functions, and publicly-accessible JSON-RPC full nodes also only support a subset of all JSON-RPC functions.

In order to do this properly, we suggest to distribute all JSON-RPC functions between groups, and nodes are allowed to support only a certain subset of groups. However, each group, if it is supported, must be supported entirely. The group a JSON-RPC function belongs to is indicated by a `prefix_` in its name.

The groups must also include a version number. For example, each function prefixed with `foo_v1_` belong to version 1 of the group `foo`. Multiple versions of the same group name might co-exist in the future. Remember that some servers might support `foo_v2_` and not `foo_v1_`, or vice versa. As such, each group+version must be "self-contained".
Functions from multiple different group+version should never be mixed. For example, `foo_v1_start` can only be stopped with `foo_v1_stop` and `foo_v2_start` can only be stopped with `foo_v2_stop`. `foo_v1_stop` must not be able to stop `foo_v2_start` and vice versa.
Functions that are unstable should always use version `unstable` of a group. For instance, `foo_unstable_`. Functions must be stable only if we are forever ready to commit to their stability. Unstable functions can break at any time, and thus more freedom is given to them.
We understand that developers want to be able to add RPC functions for various reasons, such as debugging. When doing so, they are strongly encouraged to assign functions to a group with the `_unstable` prefix.
It is completely fine to leave functions as unstable forever and never try to stabilize them. In particular, there is no drawback in leaving as unstable functions that aren't meant to be called programmatically.

JSON-RPC server should always support the `rpc_methods` function, and clients should use this function to determine which other functions are supported.

**Note**: Protocol specification formats such as https://open-rpc.org/ have been considered, but are unfortunately lacking the capabilities to describe subscriptions, and their interest is therefore limited.
