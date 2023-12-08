package beefy

import (
	"github.com/snowfork/go-substrate-rpc-client/v4/types"
	"github.com/snowfork/snowbridge/relayer/crypto/merkle"
	"github.com/snowfork/snowbridge/relayer/substrate"
)

type Request struct {
	Validators       []substrate.Authority
	SignedCommitment types.SignedCommitment
	Proof            merkle.SimplifiedMMRProof
	IsHandover       bool
}
