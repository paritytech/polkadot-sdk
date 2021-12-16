# sudo_unstable_p2pDiscover

**Parameters**:

 - `multiaddr`: String containing a text representation of [a multiaddress](https://github.com/multiformats/multiaddr) that ends with `/p2p/...`.

**Return value**: *none*

Adds an entry to the address book of peer-to-peer nodes of the JSON-RPC server.
The multiaddress passed as parameter should contain the address and identity of a node serving the libp2p protocol.

The JSON-RPC server might start connecting to this node, but it is also free to entirely ignore it.

An example of a valid multiaddress is `/ip4/10.2.83.208/tcp/30333/p2p/12D3KooWSNvfxTYrtxqAGmYM1VAtg6YMuAGWvjQ28UvoYoBBgANr`.

A JSON-RPC error should be returned if the JSON-RPC server doesn't support the protocols in the address. In this example, the JSON-RPC server should return an error if it doesn't support plain TCP connections.

Because a JSON-RPC server is also free to completely ignore the address, it is not strictly mandatory to return a JSON-RPC error when its protocols are not supported.

However, a JSON-RPC server must always return a JSON-RPC error if it couldn't parse the address. A JSON-RPC client can rely on this behavior to validate user-provided multiaddresses.

## Possible errors

- A JSON-RPC error is generated if the JSON-RPC server couldn't parse `multiaddr`.
- A JSON-RPC error is generated if the JSON-RPC server doesn't support some of the protocols in the `multiaddr`.

## About errors

It could be useful for a JSON-RPC client to be able to distinguish between addresses that are completely malformed, and would return an error on all JSON-RPC servers, and addresses that contain unsupported protocols, which could be supported by other JSON-RPC servers.

A better API for this function would consist in returning a JSON-RPC error only if the address if malformed, and a successful response containing a boolean equal to `false` if the address contains unsupported protocols.

However, this would force JSON-RPC servers to support parsing all the protocols currently defined in the multiaddress specification. Because the multiaddress specification doesn't use proper versioning and is constantly getting new protocol additions, this would be tedious to enforce.

Instead, an invalid multiaddress and an unsupported protocol lead to the same JSON-RPC error so that JSON-RPC servers only need to be able to parse the protocols they support.
