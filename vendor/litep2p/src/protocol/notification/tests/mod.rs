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

use crate::{
    executor::DefaultExecutor,
    protocol::{
        notification::{
            handle::NotificationHandle, Config as NotificationConfig, NotificationProtocol,
        },
        InnerTransportEvent, ProtocolCommand, TransportService,
    },
    transport::{
        manager::{TransportManager, TransportManagerBuilder},
        KEEP_ALIVE_TIMEOUT,
    },
    types::protocol::ProtocolName,
    PeerId,
};

use tokio::sync::mpsc::{channel, Receiver, Sender};

#[cfg(test)]
mod notification;
#[cfg(test)]
mod substream_validation;

/// create new `NotificationProtocol`
fn make_notification_protocol() -> (
    NotificationProtocol,
    NotificationHandle,
    TransportManager,
    Sender<InnerTransportEvent>,
) {
    let manager = TransportManagerBuilder::new().build();

    let peer = PeerId::random();
    let (transport_service, tx) = TransportService::new(
        peer,
        ProtocolName::from("/notif/1"),
        Vec::new(),
        std::sync::Arc::new(Default::default()),
        manager.transport_manager_handle(),
        KEEP_ALIVE_TIMEOUT,
    );
    let (config, handle) = NotificationConfig::new(
        ProtocolName::from("/notif/1"),
        1024usize,
        vec![1, 2, 3, 4],
        Vec::new(),
        false,
        64,
        64,
        true,
    );

    (
        NotificationProtocol::new(
            transport_service,
            config,
            std::sync::Arc::new(DefaultExecutor {}),
        ),
        handle,
        manager,
        tx,
    )
}

/// add new peer to `NotificationProtocol`
fn add_peer() -> (PeerId, (), Receiver<ProtocolCommand>) {
    let (_tx, rx) = channel(64);

    (PeerId::random(), (), rx)
}
