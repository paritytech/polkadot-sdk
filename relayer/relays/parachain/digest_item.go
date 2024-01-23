package parachain

import (
	"github.com/snowfork/go-substrate-rpc-client/v4/types"
)

func ExtractCommitmentFromDigest(digest types.Digest) (*types.H256, error) {
	for _, digestItem := range digest {
		if digestItem.IsOther {
			digestItemRawBytes := digestItem.AsOther
			// Prefix 0 reserved for snowbridge
			if digestItemRawBytes[0] == 0 {
				var commitment types.H256
				err := types.DecodeFromBytes(digestItemRawBytes[1:], &commitment)
				if err != nil {
					return nil, err
				}
				return &commitment, nil
			}
		}
	}
	return nil, nil
}
