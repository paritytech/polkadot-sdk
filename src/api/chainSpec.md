# Introduction

The functions with the `chainSpec` prefix allow inspecting the content of the specification of the chain a JSON-RPC server is targeting.

Because the chain specification never changes while a JSON-RPC server is running, the return value of all these functions must never change and can be cached by the JSON-RPC client.

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
