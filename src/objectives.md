# Objectives

The objective of this JSON-RPC interface is to accomodate multiple kinds of audiences:

- End-user-facing applications that want to read and interact with the blockchain.
- Node operators to want to make sure that their node is operating correctly, and perform some administrative operations such as rotating keys.
- Core/parachain developers that want to manually look at the storage and figure out what is happening on the blockchain.
- Oracles or bridges need to be able to read and interface with the blockchain.
- Archivers want to look at past data.

## End-user-facing applications

End-user-facing applications, such as a wallet or an unstoppable application, need to be able to read the storage of the blockchain and submit transactions.

These applications perform JSON-RPC function calls either against a node run locally by the end-user, or against a trusted JSON-RPC server. The locally-run node solution is strictly better for security and decentralization reasons, and we would like to encourage this. The trusted JSON-RPC server should be used as a back-up solution only in case it is not possible to run a node locally.

In order to be more user-friendly, a node run locally is very often a _light client_ that doesn't hold the storage of the blockchain in its memory. The JSON-RPC functions are designed having in mind the fact that the target of the function calls might be a light client that doesn't have all the needed information immediately available.

End-user-facing applications generally rarely need to access older blocks. They usually care only about the storage of the finalized block and of the best block.

End-user-facing applications would normally not directly use the JSON-RPC interface, but an intermediary layer library built on top of the JSON-RPC interface. The JSON-RPC interface is a bit complicated to use, in a exchange for making functions more explicit and predictable.

An end-user-facing application is typically a website that the end-user visits. Both in the case of a locally-run node and in the case of a remote node, the JSON-RPC server is subject to attacks by malicious applications as an application can ask the end-user's browser to send millions of requests to the server. For this reason, it is important for the JSON-RPC server to resist to some degree to attacks (both DoS attacks and vulnerabilities), and thus for the JSON-RPC interface to not require behaviors that contradict DoS resilience.

When calls are made against a node run locally, the bandwidth consumption and latency of the JSON-RPC functions isn't very important. However, the JSON-RPC functions have a very precise behavior, in order to avoid situations where an ambiguity in what the JSON-RPC client desires leads the JSON-RPC server to use more bandwidth than is strictly required.

When calls are made against a trusted JSON-RPC server, the bandwidth consumption and latency are more important. However, one should keep in mind that we would like to discourage trusted JSON-RPC servers.

## Node operators

DevOps that are administering a node (be it a full node, a validator, or an archive node) want to be able to know whether their node is operating properly, and might want to change some configuration options while the node is running.

DevOps are usually familiar with bash scripts. In order to make their life easier, the functions in the JSON-RPC interface that are relevant to them are usable with just a few CLI tools. The [websocat](https://github.com/vi/websocat) CLI tool is probably the easiest way to communicate over a WebSocket connection at the time of writing of this document.

DevOps shouldn't have to use unstable functions when writing bash scripts. They want to be able to run scripts automatically in the background without them breaking. As such, the functions that they need are stable.

Since scripts usually run on the same machine or a machine in the same data center as the target of the JSON-RPC function calls, the bandwidth consumption and latency of the JSON-RPC functions isn't very important for this usage.

## Debugging developers

Core/parachain developers want to be able to look at the on-chain storage or the internal state of a node, for debugging purposes.

When debugging the chain or the node implementation, developers rarely care about the stability of the function they use. They perform manual function calls using some tools (either a UI or a CLI tool) and throw away the function call once they're finished debugging.

In order to accomodate this audience, it should be relatively easy to perform a JSON-RPC function call manually, and developers should be able to easily add to the interface new semi-temporary JSON-RPC functions specific to their debugging needs.

The target of the JSON-RPC function calls is either their own node, or a specific node that has encountered an issue, and the bandwidth consumption and latency of the JSON-RPC functions isn't very important for this usage.

## Oracles and bridges

Oracles and bridges consist for example in cryptocurencies exchanges or any software that programmatically interacts with a chain.

Contrary to end-user-facing applications, no human is interfacing directly or indirectly with the JSON-RPC client. Everything is automatic.
However, everything said in the section about end-user-facing applications also applies here.

## Archivers

Archivers are websites or applications that want to look at the past state of the chain.

In order to accomodate this audience, the JSON-RPC interface should provide functions that lets you access any block of the chain, and the storage of the chain at any block.
Note that pruned blocks, in other words blocks that aren't descendants of the latest finalized blocks, are out of scope of this use case as inspecting them isn't considered useful for archiving purposes.

Apart from this difference, this category is a mix between "end-user-facing applications" and "node operators".

Archivers expect the JSON-RPC function calls to be stable and easy to use. They normally don't pay too much attention to performances and don't optimize their code, as long as everything is relatively fast.
