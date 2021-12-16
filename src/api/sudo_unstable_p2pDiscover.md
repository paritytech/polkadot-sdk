# sudo_unstable_p2pDiscover

**Parameters**:

 - `multiaddr`: String containing a text representation of [a multiaddress](https://github.com/multiformats/multiaddr) that ends with `/p2p/...`.

**Return value**: *none*

Adds an entry to the address book of peer-to-peer nodes of the JSON-RPC server.
The multiaddress passed as parameter should contain the address and identity of a node serving the libp2p protocol.

The JSON-RPC server might start connecting to this node, but it is also free to entirely ignore it.

An example of a valid multiaddress is `/ip4/10.2.83.208/tcp/30333/p2p/12D3KooWSNvfxTYrtxqAGmYM1VAtg6YMuAGWvjQ28UvoYoBBgANr`.
A JSON-RPC error should be returned if the JSON-RPC server doesn't support the protocols in the address. In this example, the JSON-RPC server should return an error if it doesn't support plain TCP connections.

**Note**: A better API for this function would consist in returning a JSON-RPC error only if the address if malformed, and a successful response containing a boolean equal to `false` if the address contains unrecognized protocols. However, this would force JSON-RPC servers to support parsing all the protocols currently defined in the multiaddress specification. Because the multiaddress specification doesn't use any versioning and is constantly getting new protocols, this would be impossible to enforce. Instead, an unsupported protocol and an unrecognized protocol lead to the same JSON-RPC error so that JSON-RPC servers only need to be able to parse the protocols they support.

## Possible errors

- A JSON-RPC error is generated if the `multiaddr` isn't a valid multiaddr.
- A JSON-RPC error is generated if the JSON-RPC server doesn't support some of the protocols in the `multiaddr`.
