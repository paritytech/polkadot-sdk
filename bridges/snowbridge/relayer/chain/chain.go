// Copyright 2020 Snowfork
// SPDX-License-Identifier: LGPL-3.0-only

package chain

import "github.com/ethereum/go-ethereum/common"

type Message interface{}

// Message from ethereum
type EthereumOutboundMessage struct {
	Origin common.Address
	Nonce  uint64
}

type Header struct {
	HeaderData interface{}
	ProofData  interface{}
}
