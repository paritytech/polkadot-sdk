# chainSpec_v1_properties

**Parameters**: *none*

**Return value**: *any*

Returns the JSON payload found in the chain specification under the key `properties`. No guarantee is offered about the content of this object.

The JSON-RPC server is allowed to re-format the JSON payload found in the chain specification. In other words, whitespaces aren't necessarily preserved.

The value returned by this function must never change for the lifetime of the connection between the JSON-RPC client and server.

## Usage

Because no guarantee is offered about the nature of the returned value, this JSON-RPC function is expected to be used in contexts where the JSON-RPC client knows what the `properties` field contains.

The `properties` field is a useful way for a chain developer to store important information about their chain, such as the name of the token or the number of decimals, in the chain specification. Without this field, important constants would need to be copy-pasted across all UIs that connect to said chain, potentially leading to mistakes.
