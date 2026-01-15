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

//! This example demonstrates how application can implement transaction gossiping.
//!
//! Run: `RUST_LOG=gossiping=info cargo run --example gossiping`

use litep2p::{
    config::ConfigBuilder,
    protocol::notification::{
        Config as NotificationConfig, ConfigBuilder as NotificationConfigBuilder,
        NotificationEvent, NotificationHandle, ValidationResult,
    },
    types::protocol::ProtocolName,
    Litep2p, PeerId,
};

use futures::StreamExt;
use tokio::sync::mpsc::{channel, Receiver, Sender};

use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

/// Dummy transaction.
#[derive(Debug, Hash, PartialEq, Eq, Clone)]
struct Transaction {
    tx: Vec<u8>,
}

/// Handle which allows communicating with [`TransactionProtocol`].
struct TransactionProtocolHandle {
    tx: Sender<Transaction>,
}

impl TransactionProtocolHandle {
    /// Create new [`TransactionProtocolHandle`].
    fn new() -> (Self, Receiver<Transaction>) {
        let (tx, rx) = channel(64);

        (Self { tx }, rx)
    }

    /// Announce transaction by sending it to the [`TransactionProtocol`] which will send
    /// it to all peers who don't have it yet.
    async fn announce_transaction(&self, tx: Transaction) {
        self.tx.send(tx).await.unwrap();
    }
}

/// Transaction protocol.
struct TransactionProtocol {
    /// Notification handle used to send and receive notifications.
    tx_handle: NotificationHandle,

    /// Handle for receiving transactions from user that should be sent to connected peers.
    rx: Receiver<Transaction>,

    /// Connected peers.
    peers: HashMap<PeerId, HashSet<Transaction>>,

    /// Seen transactions.
    seen: HashSet<Transaction>,
}

impl TransactionProtocol {
    fn new() -> (Self, NotificationConfig, TransactionProtocolHandle) {
        let (tx_config, tx_handle) = Self::init_tx_announce();
        let (handle, rx) = TransactionProtocolHandle::new();

        (
            Self {
                tx_handle,
                rx,
                peers: HashMap::new(),
                seen: HashSet::new(),
            },
            tx_config,
            handle,
        )
    }

    /// Initialize notification protocol for transactions.
    fn init_tx_announce() -> (NotificationConfig, NotificationHandle) {
        NotificationConfigBuilder::new(ProtocolName::from("/notif/tx/1"))
            .with_max_size(1024usize)
            .with_handshake(vec![1, 2, 3, 4])
            .build()
    }

    /// Poll next transaction from the protocol.
    async fn next(&mut self) -> Option<(PeerId, Transaction)> {
        loop {
            tokio::select! {
                event = self.tx_handle.next() => match event? {
                    NotificationEvent::ValidateSubstream { peer, .. } => {
                        tracing::info!("inbound substream received from {peer}");

                        self.tx_handle.send_validation_result(peer, ValidationResult::Accept);
                    }
                    NotificationEvent::NotificationStreamOpened { peer, .. } => {
                        tracing::info!("substream opened for {peer}");

                        self.peers.insert(peer, HashSet::new());
                    }
                    NotificationEvent::NotificationStreamClosed { peer } => {
                        tracing::info!("substream closed for {peer}");

                        self.peers.remove(&peer);
                    }
                    NotificationEvent::NotificationReceived { peer, notification } => {
                        tracing::info!("transaction received from {peer}: {notification:?}");

                        // send transaction to all peers who don't have it yet
                        let notification = notification.freeze();

                        for (connected, txs) in &mut self.peers {
                            let not_seen = txs.insert(Transaction { tx: notification.clone().into() });
                            if connected != &peer && not_seen {
                                self.tx_handle.send_sync_notification(
                                    *connected,
                                    notification.clone().into(),
                                ).unwrap();
                            }
                        }

                        if self.seen.insert(Transaction { tx: notification.clone().into() }) {
                            return Some((peer, Transaction { tx: notification.clone().into() }))
                        }
                    }
                    _ => {}
                },
                tx = self.rx.recv() => match tx {
                    None => return None,
                    Some(transaction) => {
                        // send transaction to all peers who don't have it yet
                        self.seen.insert(transaction.clone());

                        for (peer, txs) in &mut self.peers {
                            if txs.insert(transaction.clone()) {
                                self.tx_handle.send_sync_notification(
                                    *peer,
                                    transaction.tx.clone(),
                                ).unwrap();
                            }
                        }
                    }
                }
            }
        }
    }

    /// Start event loop for [`TransactionProtocol`].
    async fn run(mut self) {
        loop {
            match self.next().await {
                Some((peer, tx)) => {
                    tracing::info!("received transaction from {peer}: {tx:?}");
                }
                None => return,
            }
        }
    }
}

async fn await_substreams(
    tx1: &mut TransactionProtocol,
    tx2: &mut TransactionProtocol,
    tx3: &mut TransactionProtocol,
    tx4: &mut TransactionProtocol,
) {
    loop {
        tokio::select! {
            _ = tx1.next() => {}
            _ = tx2.next() => {}
            _ = tx3.next() => {}
            _ = tx4.next() => {}
            _ = tokio::time::sleep(Duration::from_secs(2)) => {
                if tx1.peers.len() == 1 && tx2.peers.len() == 3 && tx3.peers.len() == 1 && tx4.peers.len() == 1 {
                    return
                }
            }
        }
    }
}

/// Initialize peer with transaction protocol enabled.
fn tx_peer() -> (Litep2p, TransactionProtocol, TransactionProtocolHandle) {
    // initialize `TransctionProtocol`
    let (tx, tx_announce_config, tx_handle) = TransactionProtocol::new();

    // build `Litep2pConfig`
    let config = ConfigBuilder::new()
        .with_tcp(Default::default())
        .with_notification_protocol(tx_announce_config)
        .build();

    // create `Litep2p` object and start internal protocol handlers and the QUIC transport
    let litep2p = Litep2p::new(config).unwrap();

    (litep2p, tx, tx_handle)
}

#[tokio::main]
async fn main() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (mut litep2p1, mut tx1, tx_handle1) = tx_peer();
    let (mut litep2p2, mut tx2, _tx_handle2) = tx_peer();
    let (mut litep2p3, mut tx3, tx_handle3) = tx_peer();
    let (mut litep2p4, mut tx4, tx_handle4) = tx_peer();

    tracing::info!("litep2p1: {}", litep2p1.local_peer_id());
    tracing::info!("litep2p2: {}", litep2p2.local_peer_id());
    tracing::info!("litep2p3: {}", litep2p3.local_peer_id());
    tracing::info!("litep2p4: {}", litep2p4.local_peer_id());

    // establish connection to litep2p for all other litep2ps
    let peer2 = *litep2p2.local_peer_id();
    let listen_address = litep2p2.listen_addresses().next().unwrap().clone();

    litep2p1.add_known_address(peer2, vec![listen_address.clone()].into_iter());
    litep2p3.add_known_address(peer2, vec![listen_address.clone()].into_iter());
    litep2p4.add_known_address(peer2, vec![listen_address].into_iter());

    tokio::spawn(async move { while let Some(_) = litep2p1.next_event().await {} });
    tokio::spawn(async move { while let Some(_) = litep2p2.next_event().await {} });
    tokio::spawn(async move { while let Some(_) = litep2p3.next_event().await {} });
    tokio::spawn(async move { while let Some(_) = litep2p4.next_event().await {} });

    // open substreams
    tx1.tx_handle.open_substream(peer2).await.unwrap();
    tx3.tx_handle.open_substream(peer2).await.unwrap();
    tx4.tx_handle.open_substream(peer2).await.unwrap();

    // wait a moment for substream to open and start `TransactionProtocol` event loops
    await_substreams(&mut tx1, &mut tx2, &mut tx3, &mut tx4).await;

    tokio::spawn(tx1.run());
    tokio::spawn(tx2.run());
    tokio::spawn(tx3.run());
    tokio::spawn(tx4.run());

    // annouce three transactions over three different handles
    tx_handle1
        .announce_transaction(Transaction {
            tx: vec![1, 2, 3, 4],
        })
        .await;

    tx_handle3
        .announce_transaction(Transaction {
            tx: vec![1, 3, 3, 7],
        })
        .await;

    tx_handle4
        .announce_transaction(Transaction {
            tx: vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
        })
        .await;

    // allow protocols to process announced transactions before exiting
    tokio::time::sleep(Duration::from_secs(3)).await;
}
