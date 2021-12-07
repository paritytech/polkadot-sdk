# Objectives

The objective of this JSON-RPC interface is to accomodate three kinds of audiences:

- End-user-facing applications that want to read and interact with a blockchain.
- Node operators to want to make sure that their node is operating correctly, and perform some administrative operations such as rotating keys.
- Core/parachain developers that want to manually look at the storage and figure out what is happening on the blockchain.

## End-user-facing applications

End-user-facing applications, such as a wallet or an unstoppable application, need to be able to read the storage of the blockchain and submit transactions. The JSON-RPC interface should help them with that.

These applications perform JSON-RPC function calls either against a node run locally by the end-user, or against a trusted JSON-RPC server. The locally-run node solution is strictly better for security and decentralization reasons, and we would like to encourage this. The trusted JSON-RPC server should be used as a back-up solution only in case it is not possible to run a node locally.

In order to be more user-friendly, a node run locally is very often a _light client_ that doesn't hold the storage of the blockchain in its memory. The JSON-RPC functions should be designed having in mind the fact that the target of the function calls might be a light client that doesn't have all the needed information immediately available.

End-user-facing applications generally rarely need to access older blocks. They usually care only about the storage of the finalized block and of the best block.

End-user-facing applications would normally not directly use the JSON-RPC interface, but an intermediary layer library built on top of the JSON-RPC interface. It is acceptable for the JSON-RPC interface to be a bit complicated to use if it makes it more explicit and predictable.

An end-user-facing application is typically a website that the end-user visits. Both in the case of a locally-run node and in the case of a remote node, the JSON-RPC server is subject to attacks by malicious applications as an application can ask the end-user's browser to send millions of requests to the server. For this reason, it is important for the JSON-RPC server to resist to some degree to attacks (both DoS attacks and vulnerabilities), and thus for the JSON-RPC interface to not require behaviors that contradict DoS resilience.

When calls are made against a node run locally, the bandwidth consumption and latency of the JSON-RPC functions isn't very important. When calls are made against a trusted JSON-RPC server, the bandwidth consumption and latency are more important. However, one should keep in mind that we would like to discourage trusted JSON-RPC servers.

## Node operators

DevOps that are administering a node want to be able to know whether their node is operating properly, and might want to change some configuration options while the node is running. The JSON-RPC interface should help them with that.

DevOps are usually familiar with bash scripts. In order to make their life easier, the functions in the JSON-RPC interface that are relevant to them should be usable with just a few CLI tools. The [websocat](https://github.com/vi/websocat) CLI tool is probably the easiest way to communicate over a WebSocket connection at the time of writing of this document.

DevOps shouldn't have to use unstable functions when writing bash scripts. They want to be able to run scripts automatically in the background without them breaking. As such, the functions they use should be stable.

Since scripts usually run on the same machine or a machine in the same data center as the target of the JSON-RPC function calls, the bandwidth consumption and latency of the JSON-RPC functions isn't very important for this usage.

## Debugging developers

Core/parachain developers want to be able to look at the on-chain storage or the internal state of a node, for debugging purposes. The JSON-RPC interface should help them with that.

When debugging the chain or the node implementation, developers rarely care about the stability of the function they use. They perform manual function calls using some tools (either a UI or a CLI tool) and throw away the function call once they're finished debugging.

In order to accomodate this audience, it should be relatively easy to perform a JSON-RPC function call manually, and developers should be able to easily add to the interface new semi-temporary JSON-RPC functions specific to their debugging needs.

The target of the JSON-RPC function calls is either their own node, or a specific node that has encountered an issue, and the bandwidth consumption and latency of the JSON-RPC functions isn't very important for this usage.
