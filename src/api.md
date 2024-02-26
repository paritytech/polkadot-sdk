# API specification

Note that all parameters are mandatory unless specified otherwise. All functions support two ways of calling them: either by passing an array with an ordered list of parameters, or by passing an object where parameters are named.

Any missing parameter, or parameter with an invalid format, should result in a JSON-RPC error being returned, as described in the JSON-RPC specification.

Any function returning an opaque subscription or operation ID ensures that this is returned before any related notifications are generated.

The functions within each respective category must be called from the same connection in order to work together.

## Glossary

- "hexadecimal-encoded" designates a binary value encoded as hexadecimal. The value must either be empty, or start with `"0x"` and contain an even number of characters.
- "SCALE-encoded" designates a value encoded using [the SCALE codec](https://docs.substrate.io/v3/advanced/scale-codec/).
- "Merkle value" is described in the [Polkadot specification](https://spec.polkadot.network/chap-state#defn-merkle-value).
