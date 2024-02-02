# rpc_methods

**Parameters**: *none*

**Return value**: A JSON object.

The JSON object returned by this function has the following format:

```
{
    "methods": [...]
}
```

Where:

- `methods` contains an array of strings indicating the names of all the JSON-RPC functions supported by the JSON-RPC server, including this one.
