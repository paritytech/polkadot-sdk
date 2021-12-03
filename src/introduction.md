# Introduction

## Grouping functions and node capabilities

Before going further, let's examine *why* we're building a JSON-RPC interface at all:

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

## Guiding principles for new functions

All the functions in the new API should conform to the following:

- camelCase and not snake_case, as this is the norm in JavaScript.
- Function names must start with a prefix indicating the category the function belongs to.
- Functions should keep in mind the fact that they might be called on a load balancer, and that requests could be distributed between multiple nodes. For example, after submitting a transaction, retrieving the list of pending transactions might return an empty list, because the submission was sent to a different node than the one the list was retrieved from.
- Functions should only generate errors in case of a communication issue between the client and server (e.g. missing parameter, unknown function), or in case of a problem with the node (e.g. processing the request triggers a panic, or the node detected that it wasn't compatible with the chain it is supposed to connect to). Functions shouldn't generate JSON-RPC errors in circumstances that can happen normally, for example the impossibility to access a storage item due to lack of connectivity. This lets a JSON-RPC client implementation treat all JSON-RPC errors as critical problems, rather than having to interpret these errors.
- Functions that produce notifications should keep in mind that it must be possible for a JSON-RPC server to drop notifications in case the client isn't processing them quickly enough. Example: let's take the `chain_subscribeNewHeads` function that reports updates about the new best block. A server can implement this by detecting when a new best block happens then trying to send a notification. After the notification has been sent, the server then checks whether the best block is still the same as the one that was notified, and if not sends a notification again. This implementation uses a fixed amount of memory, which is a good thing, but might lead to some best block updates being missed (e.g. if the best block changes multiple times while trying to send the notification). If it was mandatory for the server to report every single best block update, it would have no choice but to buffer all the updates that happen while sending a notification, which could lead to an infinite amount of memory being used. For this reason, it must not be mandatory to report every single best block update.
- No JSON-RPC function should use up a disproportionate amount of CPU power in order to be answered compared to the other functions, in order to avoid a situation where a few expensive calls in front of the queue are blocking cheap callers behind them.
- Implementations of this API should enforce a limit to the number of simultaneous subscriptions, meaning that all active subscriptions should have roughly the same CPU/memory cost for the implementation.
- It is **not** an objective to optimize the bandwidth usage of JSON-RPC client <-> server communication, as all the idiomatic usages of this API involve communicating through `localhost`. However it should still be realistic to use TCP/IP as a quick solution. Similarly, it is not an objective to minimize the number of round-trips necessary between the JSON-RPC client and server.
- The objective of this interface is to give clear, explicit, and direct access to a node's internal state, and **not** to be convenient to use. Functions that require, for example, some post-processing on the data should be avoided, and caches should preferably be found on the client side rather than the server side. High-level developers are not expected to directly use the client side of this interface, but instead to use an intermediary layer on top of this client side.
