# sudo_network_unstable_watch

**Parameters**: *none*

**Return value**: String containing an opaque value representing the subscription.

This functions lets the JSON-RPC client track the state of the peer-to-peer networking of the blockchain node associated to the JSON-RPC server.

The subscription can later be stopped by calling `sudo_network_unstable_unwatch`.

When this function is called, a `connectionState` event is generated for each connection that already exists, and a `substreamState` event is generated for each substream that already exists. In other words, the JSON-RPC server must send its current networking state to the JSON-RPC client.
In addition, the JSON-RPC server is encouraged to notify the JSON-RPC client of connections and substreams that have recently been closed.

The JSON-RPC server must accept at least one `sudo_network_unstable_watch` subscriptions per JSON-RPC client. Trying to open more might lead to a JSON-RPC error when calling `sudo_network_unstable_watch`. In other words, as long as a JSON-RPC client only starts one `sudo_network_unstable_watch` subscriptions, it is guaranteed that this return value will never happen.

## Notifications format

This function will later generate one or more notifications in the following format:

```json
{
    "jsonrpc": "2.0",
    "method": "sudo_networkState_event",
    "params": {
        "subscription": "...",
        "result": ...
    }
}
```

Where `subscription` is the value returned by this function, and `result` can be one of:

### connectionState

```json
{
    "event": "connectionState",
    "connectionId": "...",
    "targetPeerId": "...",
    "targetMultiaddr": "...",
    "status": "connecting" | "open" | "closed",
    "direction": "in" | "out",
    "when": ...
}
```

A `connectionState` event is generated when a new connection attempt is started, when a connection has finished its handshaking phase, or when a connection is terminated.

`connectionId` is an opaque string representing this specific connection.

`status` indicates the state of the connection: `connecting` if the connection hasn't finished its handshake phase (including the libp2p-specific handshakes), `open` if the connection is fully established and can open substreams, or `closed` if the connection is now dead.

Each `connectionId` must follow one of the follow `status` transitions: `connecting` then `open` then `closed`, or `connecting` then `closed` (if an error happend during the handshake). The JSON-RPC server is not allowed to omit events such as the `connecting` event.

Once a `connectionState` event with `status` equal to `closed` is generated, the `connectionId` is unallocated. Any further usage of the same `connectionId` designates a different connection instead.

If `status` is `closed`, the connection must not have any associated substream still alive. A `substreamEvent` of `status` equal to `closed` must have been generated earlier for each substream that corresponds to this connection.

If `status` is `open` or `closed`, the `targetPeerId` is a string containing the string representation of the PeerId of the remote side of the connection. If `status` is `connecting` and `direction` is `in`, the `targetPeerId` must be omitted. If `status` is `connecting`, the `targetPeerId` contains the string representation of the PeerId that the remote is expected to have, which might end up being different from the actual PeerId.

`targetMultiaddr` is a string containing the string representation of the multiaddress of the remote side of the connection. The value in the `targetMultiaddr` field must always be the same for all the events related to a specific connection. In other words, a the multiaddress of the remote never changes during the lifetime of the connection.

`direction` indicates whether the connection was initiated locally (`out`) or by the remote (`in`). The value in the `direction` field must always be the same for all the events related to a specific connection. In other words, a connection never changes direction during its lifetime.

`when` is an integer containing the UNIX timestamp in milliseconds, in other words the number of milliseconds since the UNIX epoch ignoring leap seconds.

### substreamState

```json
{
    "event": "substreamState",
    "connectionId": "...",
    "substreamId": "...",
    "status": "open" | "closed",
    "protocolName": "...",
    "direction": "in" | "out",
    "when": ...
}
```

A `substreamState` event is generated when a new connection attempt is started, when a connection has finished its handshaking phase, or when a connection is terminated.

`connectionId` is an opaque string representing this specific connection. It must always correspond to a connection whose latest `status` is `open`.

`substreamId` is an opaque string representing this specific substream within the connection. Each substream is identified by the `connectionId` + `substreamId` tuple rather than just the `substreamId` alone. The JSON-RPC server is allowed to use the same value of `substreamId` for two different substreams belonging to two different connections.

`status` indicates the state of the substream: `open` if the substream is "alive", or `closed` if the substream is dead. A substream is considered "alive" if the JSON-RPC server allocates resources for this substream, even if the remote isn't aware of said substream.

Each `substreamState` event where `status` equal to `closed` must follow a previous `substreamState` even for that same substream where `status` was `open`. In other words, the JSON-RPC server is not allowed to omit event the `open` event.

Once a `substreamState` event with `status` equal to `closed` is generated, the `substreamId` is unallocated. Any further usage of the same `substreamId` in the context of that `connectionId` designates a different substream instead.

`protocolName` is a string indicating the multistream-select protocol name that was negotiated.

`direction` indicates whether the substream was initiated locally (`out`) or by the remote (`in`). Note that this is not the same thing as the `direction` of the corresponding connection. The value in the `direction` field must always be the same for all the events related to a specific substream. In other words, a substream never changes direction during its lifetime.

`when` is an integer containing the UNIX timestamp in milliseconds, in other words the number of milliseconds since the UNIX epoch ignoring leap seconds.

### missedEvents

```json
{
    "event": "missedEvents"
}
```

The `missedEvents` event is generated in order to inform the JSON-RPC client that it has not been informed of the existence of all connections or substreams due to it being too slow to pull JSON-RPC notifications from the JSON-RPC server.

See the `Guarantee of delivery` section for more details.

## Guarantee of delivery

JSON-RPC server implementations must be aware of the fact that JSON-RPC clients might pull notifications from the JSON-RPC server at a slower rate than networking events are generated. If this function is implemented naively, a slow or malicious JSON-RPC client can cause the JSON-RPC server to allocate ever-increasing buffers, which could in turn lead to a DoS attack on the JSON-RPC server.

JSON-RPC server implementations are also allowed to completely omit events about connections and substreams whose existence is unknown to the JSON-RPC client. For example, when a connection gets closed, the JSON-RPC server is allowed to not notify the JSON-RPC client if the client wasn't yet notified of the fact that the new-closed connection existed. When that happens, the JSON-RPC server must send a `missedEvents` event to the JSON-RPC client in the nearby future.

JSON-RPC clients must be aware that they aren't guaranteed to see the list of all connections and all substreams that the peer-to-peer endpoint of the node associated to the JSON-RPC server performs. The JSON-RPC client is only guaranteed to be aware of what is currently happening.

Assuming that the number of total active `sudo_network_unstable_watch` subscriptions on any given JSON-RPC server is bounded, and that the number of total active connections and substreams is bounded, the size of the buffer of notifications to send back to JSON-RPC clients is also bounded.

## About timestamps

The `connectionState` and `substreamState` events contain a `when` field indicating when the event happened.

The JSON-RPC server isn't required to order events by the value in their `when` field. The JSON-RPC is only required to order events so that they don't lose their logical meaning. For example, when two different connections open, the JSON-RPC server can send the two `connectionState` events in any order. When a connection opens then closes, the JSON-RPC server must send a `connectionState` with a `status` equal to `open` before the `connectionState` with a `status` equal to `closed`.

## Possible errors

- A JSON-RPC error with error code `-32100` can be generated if the JSON-RPC client has already opened a `sudo_network_unstable_watch` subscription.
