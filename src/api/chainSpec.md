# chainSpec_v1_chainName

**Parameters**: *none*
**Return value**: String containing the human-readable name of the chain.

The value returned by this function must never change.

# chainSpec_v1_genesisHash

**Parameters**: *none*
**Return value**: String containing the hex-encoded hash of the genesis block of the chain.

This function is a simple getter. The JSON-RPC server is expected to keep in its memory the hash of the genesis block.

The value returned by this function must never change.

# chainSpec_v1_properties

**Parameters**: *none*
**Return value**: *any*.

Returns the JSON payload found in the chain specification under the key `properties`. No guarantee is offered about the content of this object.

The value returned by this function must never change.

**TODO**: is that bad? stronger guarantees?
