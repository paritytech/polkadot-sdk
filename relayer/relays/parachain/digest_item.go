package parachain

import (
	"github.com/snowfork/go-substrate-rpc-client/v4/types"
)

func ExtractCommitmentFromDigest(digest types.Digest) (*types.H256, error) {
	for _, digestItem := range digest {
		if digestItem.IsOther {
			var commitment types.H256
			err := types.DecodeFromBytes(digestItem.AsOther, &commitment)
			if err != nil {
				return nil, err
			}
			return &commitment, nil
		}
	}
	return nil, nil
}
