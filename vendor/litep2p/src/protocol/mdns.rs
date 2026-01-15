// Copyright 2018 Parity Technologies (UK) Ltd.
// Copyright 2023 litep2p developers
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the "Software"),
// to deal in the Software without restriction, including without limitation
// the rights to use, copy, modify, merge, publish, distribute, sublicense,
// and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

//! [Multicast DNS](https://en.wikipedia.org/wiki/Multicast_DNS) implementation.

use crate::{transport::manager::TransportManagerHandle, DEFAULT_CHANNEL_SIZE};

use futures::Stream;
use multiaddr::Multiaddr;
use rand::{distributions::Alphanumeric, Rng};
use simple_dns::{
    rdata::{RData, PTR, TXT},
    Name, Packet, PacketFlag, Question, ResourceRecord, CLASS, QCLASS, QTYPE, TYPE,
};
use socket2::{Domain, Protocol, Socket, Type};
use tokio::{
    net::UdpSocket,
    sync::mpsc::{channel, Sender},
};
use tokio_stream::wrappers::ReceiverStream;

use std::{
    collections::HashSet,
    net,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};

/// Logging target for the file.
const LOG_TARGET: &str = "litep2p::mdns";

/// IPv4 multicast address.
const IPV4_MULTICAST_ADDRESS: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);

/// IPV4 multicast port.
const IPV4_MULTICAST_PORT: u16 = 5353;

/// Service name.
const SERVICE_NAME: &str = "_p2p._udp.local";

/// Events emitted by mDNS.
// #[derive(Debug, Clone)]
pub enum MdnsEvent {
    /// One or more addresses discovered.
    Discovered(Vec<Multiaddr>),
}

/// mDNS configuration.
// #[derive(Debug)]
pub struct Config {
    /// How often the network should be queried for new peers.
    query_interval: Duration,

    /// TX channel for sending mDNS events to user.
    tx: Sender<MdnsEvent>,
}

impl Config {
    /// Create new [`Config`].
    ///
    /// Return the configuration and an event stream for receiving [`MdnsEvent`]s.
    pub fn new(
        query_interval: Duration,
    ) -> (Self, Box<dyn Stream<Item = MdnsEvent> + Send + Unpin>) {
        let (tx, rx) = channel(DEFAULT_CHANNEL_SIZE);
        (
            Self { query_interval, tx },
            Box::new(ReceiverStream::new(rx)),
        )
    }
}

/// Main mDNS object.
pub(crate) struct Mdns {
    /// Query interval.
    query_interval: tokio::time::Interval,

    /// TX channel for sending events to user.
    event_tx: Sender<MdnsEvent>,

    /// Handle to `TransportManager`.
    _transport_handle: TransportManagerHandle,

    // Username.
    username: String,

    /// Next query ID.
    next_query_id: u16,

    /// Buffer for incoming messages.
    receive_buffer: Vec<u8>,

    /// Listen addresses.
    listen_addresses: Vec<Arc<str>>,

    /// Discovered addresses.
    discovered: HashSet<Multiaddr>,
}

impl Mdns {
    /// Create new [`Mdns`].
    pub(crate) fn new(
        _transport_handle: TransportManagerHandle,
        config: Config,
        listen_addresses: Vec<Multiaddr>,
    ) -> Self {
        let mut query_interval = tokio::time::interval(config.query_interval);
        query_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        Self {
            _transport_handle,
            event_tx: config.tx,
            next_query_id: 1337u16,
            discovered: HashSet::new(),
            query_interval,
            receive_buffer: vec![0u8; 4096],
            username: rand::thread_rng()
                .sample_iter(&Alphanumeric)
                .take(32)
                .map(char::from)
                .collect(),
            listen_addresses: listen_addresses
                .into_iter()
                .map(|address| format!("dnsaddr={address}").into())
                .collect(),
        }
    }

    /// Get next query ID.
    fn next_query_id(&mut self) -> u16 {
        let query_id = self.next_query_id;
        self.next_query_id += 1;

        query_id
    }

    /// Send mDNS query on the network.
    async fn on_outbound_request(&mut self, socket: &UdpSocket) -> crate::Result<()> {
        tracing::debug!(target: LOG_TARGET, "send outbound query");

        let mut packet = Packet::new_query(self.next_query_id());

        packet.questions.push(Question {
            qname: Name::new_unchecked(SERVICE_NAME),
            qtype: QTYPE::TYPE(TYPE::PTR),
            qclass: QCLASS::CLASS(CLASS::IN),
            unicast_response: false,
        });

        socket
            .send_to(
                &packet.build_bytes_vec().expect("valid packet"),
                (IPV4_MULTICAST_ADDRESS, IPV4_MULTICAST_PORT),
            )
            .await
            .map(|_| ())
            .map_err(From::from)
    }

    /// Handle inbound query.
    fn on_inbound_request(&self, packet: Packet) -> Option<Vec<u8>> {
        tracing::debug!(target: LOG_TARGET, ?packet, "handle inbound request");

        let mut packet = Packet::new_reply(packet.id());
        let srv_name = Name::new_unchecked(SERVICE_NAME);

        packet.answers.push(ResourceRecord::new(
            srv_name.clone(),
            CLASS::IN,
            360,
            RData::PTR(PTR(Name::new_unchecked(&self.username))),
        ));

        for address in &self.listen_addresses {
            let mut record = TXT::new();
            record.add_string(address).expect("valid string");

            packet.additional_records.push(ResourceRecord {
                name: Name::new_unchecked(&self.username),
                class: CLASS::IN,
                ttl: 360,
                rdata: RData::TXT(record),
                cache_flush: false,
            });
        }

        Some(packet.build_bytes_vec().expect("valid packet"))
    }

    /// Handle inbound response.
    fn on_inbound_response(&self, packet: Packet) -> Vec<Multiaddr> {
        tracing::debug!(target: LOG_TARGET, "handle inbound response");

        let names = packet
            .answers
            .iter()
            .filter_map(|answer| {
                if answer.name != Name::new_unchecked(SERVICE_NAME) {
                    return None;
                }

                match answer.rdata {
                    RData::PTR(PTR(ref name)) if name != &Name::new_unchecked(&self.username) =>
                        Some(name),
                    _ => None,
                }
            })
            .collect::<Vec<&Name>>();

        let name = match names.len() {
            0 => return Vec::new(),
            _ => {
                tracing::debug!(
                    target: LOG_TARGET,
                    ?names,
                    "response name"
                );

                names[0]
            }
        };

        packet
            .additional_records
            .iter()
            .flat_map(|record| {
                if &record.name != name {
                    return vec![];
                }

                // TODO: https://github.com/paritytech/litep2p/issues/333
                // `filter_map` is not necessary as there's at most one entry
                match &record.rdata {
                    RData::TXT(text) => text
                        .attributes()
                        .iter()
                        .filter_map(|(_, address)| {
                            address.as_ref().and_then(|inner| inner.parse().ok())
                        })
                        .collect(),
                    _ => vec![],
                }
            })
            .collect()
    }

    /// Setup the socket.
    fn setup_socket() -> crate::Result<UdpSocket> {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        socket.set_reuse_address(true)?;
        #[cfg(unix)]
        socket.set_reuse_port(true)?;
        socket.bind(
            &SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), IPV4_MULTICAST_PORT).into(),
        )?;
        socket.set_multicast_loop_v4(true)?;
        socket.set_multicast_ttl_v4(255)?;
        socket.join_multicast_v4(&IPV4_MULTICAST_ADDRESS, &Ipv4Addr::UNSPECIFIED)?;
        socket.set_nonblocking(true)?;

        UdpSocket::from_std(net::UdpSocket::from(socket)).map_err(Into::into)
    }

    /// Event loop for [`Mdns`].
    pub(crate) async fn start(mut self) {
        tracing::debug!(target: LOG_TARGET, "starting mdns event loop");

        let mut socket_opt = None;

        loop {
            let socket = match socket_opt.take() {
                Some(s) => s,
                None => {
                    let _ = self.query_interval.tick().await;
                    match Self::setup_socket() {
                        Ok(s) => s,
                        Err(error) => {
                            tracing::debug!(
                                target: LOG_TARGET,
                                ?error,
                                "failed to setup mDNS socket, will try again"
                            );
                            continue;
                        }
                    }
                }
            };

            tokio::select! {
                _ = self.query_interval.tick() => {
                    tracing::trace!(target: LOG_TARGET, "query interval ticked");

                    if let Err(error) = self.on_outbound_request(&socket).await {
                        tracing::debug!(target: LOG_TARGET, ?error, "failed to send mdns query");
                        // Let's recreate the socket
                        continue;
                    }
                },

                result = socket.recv_from(&mut self.receive_buffer) => match result {
                    Ok((nread, address)) => match Packet::parse(&self.receive_buffer[..nread]) {
                        Ok(packet) => match packet.has_flags(PacketFlag::RESPONSE) {
                            true => {
                                let to_forward = self.on_inbound_response(packet).into_iter().filter_map(|address| {
                                    self.discovered.insert(address.clone()).then_some(address)
                                })
                                .collect::<Vec<_>>();

                                if !to_forward.is_empty() {
                                    let _ = self.event_tx.send(MdnsEvent::Discovered(to_forward)).await;
                                }
                            }
                            false => if let Some(response) = self.on_inbound_request(packet) {
                                if let Err(error) = socket
                                    .send_to(&response, (IPV4_MULTICAST_ADDRESS, IPV4_MULTICAST_PORT))
                                    .await {
                                    tracing::debug!(target: LOG_TARGET, ?error, "failed to send mdns response");
                                    // Let's recreate the socket
                                    continue;
                                }
                            }
                        }
                        Err(error) => tracing::debug!(
                            target: LOG_TARGET,
                            ?address,
                            ?error,
                            ?nread,
                            "failed to parse mdns packet"
                        ),
                    }
                    Err(error) => {
                        tracing::debug!(target: LOG_TARGET, ?error, "failed to read from socket");
                        // Let's recreate the socket
                        continue;
                    }
                },
            };

            socket_opt = Some(socket);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::manager::TransportManagerBuilder;
    use futures::StreamExt;
    use multiaddr::Protocol;

    #[tokio::test]
    async fn mdns_works() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let (config1, mut stream1) = Config::new(Duration::from_secs(5));
        let manager1 = TransportManagerBuilder::new().build();

        let mdns1 = Mdns::new(
            manager1.transport_manager_handle(),
            config1,
            vec![
                "/ip6/::1/tcp/8888/p2p/12D3KooWNP463TyS3vUpmekjjZ2dg7xy1WHNMM7MqfsMevMTaaaa"
                    .parse()
                    .unwrap(),
                "/ip4/127.0.0.1/tcp/8888/p2p/12D3KooWNP463TyS3vUpmekjjZ2dg7xy1WHNMM7MqfsMevMTaaaa"
                    .parse()
                    .unwrap(),
            ],
        );

        let (config2, mut stream2) = Config::new(Duration::from_secs(5));
        let manager2 = TransportManagerBuilder::new().build();

        let mdns2 = Mdns::new(
            manager2.transport_manager_handle(),
            config2,
            vec![
                "/ip6/::1/tcp/9999/p2p/12D3KooWNP463TyS3vUpmekjjZ2dg7xy1WHNMM7MqfsMevMTbbbb"
                    .parse()
                    .unwrap(),
                "/ip4/127.0.0.1/tcp/9999/p2p/12D3KooWNP463TyS3vUpmekjjZ2dg7xy1WHNMM7MqfsMevMTbbbb"
                    .parse()
                    .unwrap(),
            ],
        );

        tokio::spawn(mdns1.start());
        tokio::spawn(mdns2.start());

        let mut peer1_discovered = false;
        let mut peer2_discovered = false;

        while !peer1_discovered && !peer2_discovered {
            tokio::select! {
                event = stream1.next() => match event.unwrap() {
                    MdnsEvent::Discovered(addrs) => {
                        if addrs.len() == 2 {
                            let mut iter = addrs[0].iter();

                            if !std::matches!(iter.next(), Some(Protocol::Ip4(_) | Protocol::Ip6(_))) {
                                continue
                            }

                            match iter.next() {
                                Some(Protocol::Tcp(port)) => {
                                    if port != 9999 {
                                        continue
                                    }
                                }
                                _ => continue,
                            }

                            peer1_discovered = true;
                        }
                    }
                },
                event = stream2.next() => match event.unwrap() {
                    MdnsEvent::Discovered(addrs) => {
                        if addrs.len() == 2 {
                            let mut iter = addrs[0].iter();

                            if !std::matches!(iter.next(), Some(Protocol::Ip4(_) | Protocol::Ip6(_))) {
                                continue
                            }

                            match iter.next() {
                                Some(Protocol::Tcp(port)) => {
                                    if port != 8888 {
                                        continue
                                    }
                                }
                                _ => continue,
                            }

                            peer2_discovered = true;
                        }
                    }
                }
            }
        }
    }
}
