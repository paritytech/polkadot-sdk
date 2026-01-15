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

//! Connection-related helper code.

use crate::{
    error::{Error, SubstreamError},
    protocol::protocol_set::ProtocolCommand,
    types::{protocol::ProtocolName, ConnectionId, SubstreamId},
};

use tokio::sync::mpsc::{error::TrySendError, Sender, WeakSender};

/// Connection type, from the point of view of the protocol.
#[derive(Debug, Clone)]
enum ConnectionType {
    /// Connection is actively kept open.
    Active(Sender<ProtocolCommand>),

    /// Connection is considered inactive as far as the protocol is concerned
    /// and if no substreams are being opened and no protocol is interested in
    /// keeping the connection open, it will be closed.
    Inactive(WeakSender<ProtocolCommand>),
}

/// Type representing a handle to connection which allows protocols to communicate with the
/// connection.
#[derive(Debug, Clone)]
pub struct ConnectionHandle {
    /// Connection type.
    connection: ConnectionType,

    /// Connection ID.
    connection_id: ConnectionId,
}

impl ConnectionHandle {
    /// Create new [`ConnectionHandle`].
    ///
    /// By default the connection is set as `Active` to give protocols time to open a substream if
    /// they wish.
    pub fn new(connection_id: ConnectionId, connection: Sender<ProtocolCommand>) -> Self {
        Self {
            connection_id,
            connection: ConnectionType::Active(connection),
        }
    }

    /// Get active sender from the [`ConnectionHandle`] and then downgrade it to an inactive
    /// connection.
    ///
    /// This function is only called once when the connection is established to remote peer and that
    /// one time the connection type must be `Active`, unless there is a logic bug in `litep2p`.
    pub fn downgrade(&mut self) -> Self {
        match &self.connection {
            ConnectionType::Active(connection) => {
                let handle = Self::new(self.connection_id, connection.clone());
                self.connection = ConnectionType::Inactive(connection.downgrade());

                handle
            }
            ConnectionType::Inactive(_) => {
                panic!("state mismatch: tried to downgrade an inactive connection")
            }
        }
    }

    /// Get reference to connection ID.
    pub fn connection_id(&self) -> &ConnectionId {
        &self.connection_id
    }

    /// Mark connection as closed.
    pub fn close(&mut self) {
        if let ConnectionType::Active(connection) = &self.connection {
            self.connection = ConnectionType::Inactive(connection.downgrade());
        }
    }

    /// Try to upgrade the connection to active state.
    pub fn try_upgrade(&mut self) {
        if let ConnectionType::Inactive(inactive) = &self.connection {
            if let Some(active) = inactive.upgrade() {
                self.connection = ConnectionType::Active(active);
            }
        }
    }

    /// Attempt to acquire permit which will keep the connection open for indefinite time.
    pub fn try_get_permit(&self) -> Option<Permit> {
        match &self.connection {
            ConnectionType::Active(active) => Some(Permit::new(active.clone())),
            ConnectionType::Inactive(inactive) => Some(Permit::new(inactive.upgrade()?)),
        }
    }

    /// Open substream to remote peer over `protocol` and send the acquired permit to the
    /// transport so it can be given to the opened substream.
    pub fn open_substream(
        &mut self,
        protocol: ProtocolName,
        fallback_names: Vec<ProtocolName>,
        substream_id: SubstreamId,
        permit: Permit,
    ) -> Result<(), SubstreamError> {
        match &self.connection {
            ConnectionType::Active(active) => active.clone(),
            ConnectionType::Inactive(inactive) =>
                inactive.upgrade().ok_or(SubstreamError::ConnectionClosed)?,
        }
        .try_send(ProtocolCommand::OpenSubstream {
            protocol: protocol.clone(),
            fallback_names,
            substream_id,
            connection_id: self.connection_id,
            permit,
        })
        .map_err(|error| match error {
            TrySendError::Full(_) => SubstreamError::ChannelClogged,
            TrySendError::Closed(_) => SubstreamError::ConnectionClosed,
        })
    }

    /// Force close connection.
    pub fn force_close(&mut self) -> crate::Result<()> {
        match &self.connection {
            ConnectionType::Active(active) => active.clone(),
            ConnectionType::Inactive(inactive) =>
                inactive.upgrade().ok_or(Error::ConnectionClosed)?,
        }
        .try_send(ProtocolCommand::ForceClose)
        .map_err(|error| match error {
            TrySendError::Full(_) => Error::ChannelClogged,
            TrySendError::Closed(_) => Error::ConnectionClosed,
        })
    }

    /// Check if the connection is active.
    pub fn is_active(&self) -> bool {
        matches!(self.connection, ConnectionType::Active(_))
    }
}

/// Type which allows the connection to be kept open.
#[derive(Debug, Clone)]
pub struct Permit {
    /// Active connection.
    _connection: Sender<ProtocolCommand>,
}

impl Permit {
    /// Create new [`Permit`] which allows the connection to be kept open.
    pub fn new(_connection: Sender<ProtocolCommand>) -> Self {
        Self { _connection }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc::channel;

    #[test]
    #[should_panic]
    fn downgrade_inactive_connection() {
        let (tx, _rx) = channel(1);
        let mut handle = ConnectionHandle::new(ConnectionId::new(), tx);

        let mut new_handle = handle.downgrade();
        assert!(std::matches!(
            new_handle.connection,
            ConnectionType::Inactive(_)
        ));

        // try to downgrade an already-downgraded connection
        let _handle = new_handle.downgrade();
    }

    #[tokio::test]
    async fn open_substream_open_downgraded_connection() {
        let (tx, mut rx) = channel(1);
        let mut handle = ConnectionHandle::new(ConnectionId::new(), tx);
        let mut handle = handle.downgrade();
        let permit = handle.try_get_permit().unwrap();

        let result = handle.open_substream(
            ProtocolName::from("/protocol/1"),
            Vec::new(),
            SubstreamId::new(),
            permit,
        );

        assert!(result.is_ok());
        assert!(rx.recv().await.is_some());
    }

    #[tokio::test]
    async fn open_substream_closed_downgraded_connection() {
        let (tx, _rx) = channel(1);
        let mut handle = ConnectionHandle::new(ConnectionId::new(), tx);
        let mut handle = handle.downgrade();
        let permit = handle.try_get_permit().unwrap();
        drop(_rx);

        let result = handle.open_substream(
            ProtocolName::from("/protocol/1"),
            Vec::new(),
            SubstreamId::new(),
            permit,
        );

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn open_substream_channel_clogged() {
        let (tx, _rx) = channel(1);
        let mut handle = ConnectionHandle::new(ConnectionId::new(), tx);
        let mut handle = handle.downgrade();
        let permit = handle.try_get_permit().unwrap();

        let result = handle.open_substream(
            ProtocolName::from("/protocol/1"),
            Vec::new(),
            SubstreamId::new(),
            permit,
        );
        assert!(result.is_ok());

        let permit = handle.try_get_permit().unwrap();
        match handle.open_substream(
            ProtocolName::from("/protocol/1"),
            Vec::new(),
            SubstreamId::new(),
            permit,
        ) {
            Err(SubstreamError::ChannelClogged) => {}
            error => panic!("invalid error: {error:?}"),
        }
    }
}
