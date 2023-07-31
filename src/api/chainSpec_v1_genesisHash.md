# chainSpec_v1_genesisHash

**Parameters**: *none*

**Return value**: String containing the hexadecimal-encoded hash of the header of the genesis block of the chain.

This function is a simple getter. The JSON-RPC server is expected to keep in its memory the hash of the genesis block.

The value returned by this function must never change for the lifetime of the connection between the JSON-RPC client and server.
