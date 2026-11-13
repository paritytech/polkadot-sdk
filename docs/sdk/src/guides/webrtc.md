# WebRTC Transport for Light Clients

In-browser light clients use the WebRTC transport to connect to Polkadot full nodes. Unlike WSS, it
needs no domain name, TLS certificate, or reverse proxy.

WebRTC is only supported by the litep2p network backend (the default). Light client connections
count towards the `--in-peers-light` limit (500 by default).

## 1. Listen Addresses

Unless you pass `--listen-addr`, a full node listens for incoming WebRTC connections on UDP port
30333, or on the port specified with `--port`. A parachain node runs two network stacks and accepts
WebRTC on both: by default UDP 30333 on the parachain side and 30334 on the relay chain side. Relay
chain options go after `--`.

Validators and collators, on either side, don't listen on WebRTC by default and should serve light
clients from separate full nodes instead. To disable WebRTC on a full node, pass an explicit
`--listen-addr` with TCP/WS addresses only. Don't just block the UDP port: the node would still
advertise its WebRTC address, and light clients would fail to connect.

If you manually specify `--listen-addr` for TCP/WS/WSS connections, make sure to also include a
`--listen-addr` for WebRTC connections, e.g. `/ip4/0.0.0.0/udp/30333/webrtc-direct` or
`/ip6/::/udp/30333/webrtc-direct`.

## 2. Public Addresses

If your public address or port differs from the local listen address or port (e.g., the node is
behind NAT or you redirect the ports), make sure to include a matching `--public-addr` of the form
`/ip4/203.0.113.1/udp/30333/webrtc-direct`. Substitute your public address and port, replacing
`ip4` with `ip6` or `dns` if needed. This address will be advertised in the DHT, allowing light
clients to discover it.

Don't append `/certhash/...` or `/p2p/...` to WebRTC listen or public addresses: the node adds them
itself and refuses to start if they are present.

## 3. Firewall / Port Forwarding

Make sure the UDP port is open in your firewall. Existing rules for the TCP port don't cover UDP.
If the node is behind NAT, forward the public address and port from step 2 to the local listen
address and port from step 1.

## Checking It Works

At startup the node logs `WebRTC certhash: uEi...`. The `system_localListenAddresses` RPC call
lists the WebRTC listen addresses with the certhash and peer ID appended, e.g.
`/ip4/10.0.0.1/udp/30333/webrtc-direct/certhash/uEi.../p2p/12D3Koo...`.

You can probe the port forwarding from the outside using a STUN binding request:

```sh
HOST=203.0.113.1
PORT=30333
echo 000100382112a4427765627274635f70726f62650006000b70726f62653a70726f6265000024 \
     00047e0004ff00080014476a2f17c8dcec9c701b44b6960ab7818517ed0380280004ca66c339 \
  | xxd -r -p | nc -u -w2 $HOST $PORT | head -c 20 | xxd -p \
  | grep -Eq '^0101....2112a4427765627274635f70726f6265$' && echo "WebRTC reachable"
```

The pattern matches a STUN success response with the correct transaction ID, so the message is
printed only if the packet made it there and back. No output means the WebRTC endpoint is not
reachable.

## Bootnodes in the Chainspec

For light clients to bootstrap over WebRTC, network operators need to add the bootnodes' WebRTC
addresses to `bootNodes` in the chainspec, alongside the existing TCP/WS/WSS ones. The address has
the form `/dns/<host>/udp/<port>/webrtc-direct/certhash/<certhash>/p2p/<peer id>`. Take the
certhash and peer ID from the RPC or console output above. Both are derived from the node key, so
they only change if the key does.
