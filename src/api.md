# API specification

Note that all parameters are mandatory unless specified otherwise. All functions support two ways of calling them: either by passing an array with an ordered list of parameters, or by passing an object where parameters are named.

Any missing parameter, or parameter with an invalid format, should result in a JSON-RPC error being returned, as described in the JSON-RPC specification.

## Glossary

- "hexadecimal-encoded" designates a binary value encoded as hexadecimal. The value must either be empty, or start with `"0x"` and contain an even number of characters.
- "SCALE-encoded" designates a value encoded using [the SCALE codec](https://docs.substrate.io/v3/advanced/scale-codec/).
