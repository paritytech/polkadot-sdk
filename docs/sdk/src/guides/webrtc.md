# WebRTC Transport for Light Clients

In-browser light clients use WebRTC transport for connecting to Polkadot full nodes. This guide explains how to setup a full node to handle light client connections.

## 1. Listen Addresses

A Polkadot full node automatically listens for incoming WebRTC connections on UDP port 30333 (or a custom port specified with `--port` if you don't specify any `--listen-addr`). If you manually specify `--listen-addr` for TCP/WS/WSS connections, make sure to also include a `--listen-addr` for WebRTC connections: `/ip4/10.0.0.1/udp/30333/webrtc-direct`, substituting your IP address and port (`/ip6/...` addresses are also supported.

## 2. Public Addresses

If your public address/port are different from the local listen address/port (e.g., the node is behind NAT or you redirect the ports), make sure to include a matching `--public-addr` of the form `/ip4/8.8.8.8/udp/30333/webrtc-direct`. Substitute your public address/port, replacing `ip4` with `ip6`/`dns` if needed. This address will be advertised in the DHT, allowing light clients to discover it.

## 3. Firewall / Port Forwarding

Make sure the UDP port is open in your firewall and/or the connections to the public WebRTC address / UDP port specified in #2 are forwarded to the local listen address / port used in #1.
