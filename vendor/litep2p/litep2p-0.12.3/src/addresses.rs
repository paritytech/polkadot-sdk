// Copyright 2024 litep2p developers
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

use std::{collections::HashSet, sync::Arc};

use multiaddr::{Multiaddr, Protocol};
use parking_lot::RwLock;

use crate::PeerId;

/// Set of the public addresses of the local node.
///
/// The format of the addresses stored in the set contain the local peer ID.
/// This requirement is enforced by the [`PublicAddresses::add_address`] method,
/// that will add the local peer ID to the address if it is missing.
///
/// # Note
///
/// - The addresses are reported to the identify protocol and are used by other nodes to establish a
///   connection with the local node.
///
/// - Users must ensure that the addresses are reachable from the network.
#[derive(Debug, Clone)]
pub struct PublicAddresses {
    pub(crate) inner: Arc<RwLock<HashSet<Multiaddr>>>,
    local_peer_id: PeerId,
}

impl PublicAddresses {
    /// Creates new [`PublicAddresses`] from the given peer ID.
    pub(crate) fn new(local_peer_id: PeerId) -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashSet::new())),
            local_peer_id,
        }
    }

    /// Add a public address to the list of addresses.
    ///
    /// The address must contain the local peer ID, otherwise an error is returned.
    /// In case the address does not contain any peer ID, it will be added.
    ///
    /// Returns true if the address was added, false if it was already present.
    pub fn add_address(&self, address: Multiaddr) -> Result<bool, InsertionError> {
        let address = ensure_local_peer(address, self.local_peer_id)?;
        Ok(self.inner.write().insert(address))
    }

    /// Remove the exact public address.
    ///
    /// The provided address must contain the local peer ID.
    pub fn remove_address(&self, address: &Multiaddr) -> bool {
        self.inner.write().remove(address)
    }

    /// Returns a vector of the available listen addresses.
    pub fn get_addresses(&self) -> Vec<Multiaddr> {
        self.inner.read().iter().cloned().collect()
    }
}

/// Check if the address contains the local peer ID.
///
/// If the address does not contain any peer ID, it will be added.
fn ensure_local_peer(
    mut address: Multiaddr,
    local_peer_id: PeerId,
) -> Result<Multiaddr, InsertionError> {
    if address.is_empty() {
        return Err(InsertionError::EmptyAddress);
    }

    // Verify the peer ID from the address corresponds to the local peer ID.
    if let Some(peer_id) = PeerId::try_from_multiaddr(&address) {
        if peer_id != local_peer_id {
            return Err(InsertionError::DifferentPeerId);
        }
    } else {
        address.push(Protocol::P2p(local_peer_id.into()));
    }

    Ok(address)
}

/// The error returned when an address cannot be inserted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertionError {
    /// The address is empty.
    EmptyAddress,
    /// The address contains a different peer ID than the local peer ID.
    DifferentPeerId,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn add_remove_contains() {
        let peer_id = PeerId::random();
        let addresses = PublicAddresses::new(peer_id);
        let address = Multiaddr::from_str("/dns/domain1.com/tcp/30333").unwrap();
        let peer_address = Multiaddr::from_str("/dns/domain1.com/tcp/30333")
            .unwrap()
            .with(Protocol::P2p(peer_id.into()));

        assert!(!addresses.get_addresses().contains(&address));

        assert!(addresses.add_address(address.clone()).unwrap());
        // Adding the address a second time returns Ok(false).
        assert!(!addresses.add_address(address.clone()).unwrap());

        assert!(!addresses.get_addresses().contains(&address));
        assert!(addresses.get_addresses().contains(&peer_address));

        addresses.remove_address(&peer_address);
        assert!(!addresses.get_addresses().contains(&peer_address));
    }

    #[test]
    fn get_addresses() {
        let peer_id = PeerId::random();
        let addresses = PublicAddresses::new(peer_id);
        let address1 = Multiaddr::from_str("/dns/domain1.com/tcp/30333").unwrap();
        let address2 = Multiaddr::from_str("/dns/domain2.com/tcp/30333").unwrap();
        // Addresses different than the local peer ID are ignored.
        let address3 = Multiaddr::from_str(
            "/dns/domain2.com/tcp/30333/p2p/12D3KooWSueCPH3puP2PcvqPJdNaDNF3jMZjtJtDiSy35pWrbt5h",
        )
        .unwrap();

        assert!(addresses.add_address(address1.clone()).unwrap());
        assert!(addresses.add_address(address2.clone()).unwrap());
        addresses.add_address(address3.clone()).unwrap_err();

        let addresses = addresses.get_addresses();
        assert_eq!(addresses.len(), 2);
        assert!(addresses.contains(&address1.with(Protocol::P2p(peer_id.into()))));
        assert!(addresses.contains(&address2.with(Protocol::P2p(peer_id.into()))));
    }
}
