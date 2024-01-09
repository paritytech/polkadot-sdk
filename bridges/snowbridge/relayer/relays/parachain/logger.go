package parachain

import (
	"fmt"

	log "github.com/sirupsen/logrus"
	"github.com/snowfork/go-substrate-rpc-client/v4/types"
	"github.com/snowfork/snowbridge/relayer/contracts"
)

func Hex(b []byte) string {
	return types.HexEncodeToString(b)
}

func (wr *EthereumWriter) logFieldsForSubmission(
	message contracts.InboundMessage,
	messageProof [][32]byte,
	proof contracts.VerificationProof,
) log.Fields {
	messageProofHexes := make([]string, len(messageProof))
	for i, proof := range messageProof {
		messageProofHexes[i] = Hex(proof[:])
	}

	digestItems := make([]log.Fields, len(proof.Header.DigestItems))
	for i, digestItem := range proof.Header.DigestItems {
		digestItems[i] = log.Fields{
			"kind":              digestItem.Kind,
			"consensusEngineID": digestItem.ConsensusEngineID,
			"data":              Hex(digestItem.Data),
		}
	}

	headProofHexes := make([]string, len(proof.HeadProof.Proof))
	for i, proof := range proof.HeadProof.Proof {
		headProofHexes[i] = Hex(proof[:])
	}

	mmrLeafProofHexes := make([]string, len(proof.LeafProof))
	for i, proof := range proof.LeafProof {
		mmrLeafProofHexes[i] = Hex(proof[:])
	}

	params := log.Fields{
		"message": log.Fields{
			"channelID": Hex(message.ChannelID[:]),
			"nonce":     message.Nonce,
			"command":   message.Command,
			"params":    Hex(message.Params),
		},
		"messageProof": messageProofHexes,
		"proof": log.Fields{
			"header": log.Fields{
				"parentHash":     Hex(proof.Header.ParentHash[:]),
				"number":         proof.Header.Number,
				"stateRoot":      Hex(proof.Header.StateRoot[:]),
				"extrinsicsRoot": Hex(proof.Header.ExtrinsicsRoot[:]),
				"digestItems":    digestItems,
			},
			"headProof": log.Fields{
				"pos":   proof.HeadProof.Pos,
				"width": proof.HeadProof.Width,
				"proof": headProofHexes,
			},
			"leafPartial": log.Fields{
				"version":              proof.LeafPartial.Version,
				"parentNumber":         proof.LeafPartial.ParentNumber,
				"parentHash":           Hex(proof.LeafPartial.ParentHash[:]),
				"nextAuthoritySetID":   proof.LeafPartial.NextAuthoritySetID,
				"nextAuthoritySetLen":  proof.LeafPartial.NextAuthoritySetLen,
				"nextAuthoritySetRoot": Hex(proof.LeafPartial.NextAuthoritySetRoot[:]),
			},
			"leafProof":      mmrLeafProofHexes,
			"leafProofOrder": fmt.Sprintf("%b", proof.LeafProofOrder),
		},
	}

	return params
}
