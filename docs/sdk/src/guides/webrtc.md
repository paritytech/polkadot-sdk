# WebRTC Transport for Light Clients

In-browser light clients use the WebRTC transport to connect to Polkadot full nodes. This guide
explains how to set up a full node to accept light client connections. WebRTC is only supported by
the litep2p network backend (the default).

## 1. Listen Addresses

Unless you pass `--listen-addr`, a full node listens for incoming WebRTC connections on UDP port
30333, or on the port specified with `--port`. Validators and collators don't listen on WebRTC by
default; pass `--force-enable-webrtc` to enable it.

If you manually specify `--listen-addr` for TCP/WS/WSS connections, make sure to also include a
`--listen-addr` for WebRTC connections: `/ip4/10.0.0.1/udp/30333/webrtc-direct`, substituting your
IP address and port. `/ip6/...` addresses and the wildcard `0.0.0.0` / `[::]` also work.

## 2. Public Addresses

If your public address or port differs from the local listen address or port (e.g., the node is
behind NAT or you redirect the ports), make sure to include a matching `--public-addr` of the form
`/ip4/203.0.113.1/udp/30333/webrtc-direct`. Substitute your public address and port, replacing
`ip4` with `ip6` or `dns` if needed. This address will be advertised in the DHT, allowing light
clients to discover it.

Don't append `/certhash/...` or `/p2p/...` to WebRTC listen or public addresses: the node adds them
itself and refuses to start if they are present. It also refuses to start if a public WebRTC
address is given without a WebRTC listen address.

## 3. Firewall / Port Forwarding

Make sure the UDP port is open in your firewall. Existing rules for the TCP port don't cover UDP.
If the node is behind NAT, forward the public address and port from step 2 to the local listen
address and port from step 1.

## Bootnodes in the Chainspec

For light clients to bootstrap over WebRTC, network operators need to add the bootnodes' WebRTC
addresses to `bootNodes` in the chainspec, alongside the existing TCP/WS/WSS ones. The address has
the form `/dns/<host>/udp/<port>/webrtc-direct/certhash/<certhash>/p2p/<peer id>`. The node prints
both values at startup, as `WebRTC certhash: uEi...` and `Local node identity is: 12D3Koo...`. Both
are derived from the node key, so they only change if the key does.
