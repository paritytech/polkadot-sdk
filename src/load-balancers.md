# Load balancers

In case of a publicly-accessible JSON-RPC server, it is unlikely for a single server to be able to handle the load of all the end users running JSON-RPC clients.

For this reason, the JSON-RPC interface is suitable for load balancer. A single load balancer can serve all the requests and dispatch them to multiple nodes.

With the exception of the JSON-RPC functions prefixed with ̀`sudo`, none of the JSON-RPC functions require stick sessions. In other words, if a JSON-RPC client disconnects from the load balancer, then reconnects, the load balancer can wire the connection to a different underlying server than the one it previously used. However, as long as a JSON-RPC client is connected, all its messages must be redirected to the same underlying JSON-RPC server. The underlying server can change only after a disconnect/reconnect.

## Unsafe JSON-RPC functions

In the legacy JSON-RPC interface, a flag on the server renders some JSON-RPC functions inaccessible. This is important in order to prevent the general public from accessing the node's configuration and sensitive data. In this new JSON-RPC interface, all the functions that shouldn't be publicly-accessible are prefixed with `sudo`.

This makes it possible to insert proxies that filter incoming requests. All such proxy has to do is parse JSON-RPC requests and detect whether the function name starts with a prefix that isn't `sudo_`.

In practice, however, such proxy doesn't seem to exist, and JSON-RPC server implementations should continue to provide a configuration option (such as a CLI flag) to disable `sudo`-prefixed functions.
