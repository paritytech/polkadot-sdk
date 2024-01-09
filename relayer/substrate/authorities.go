package substrate

import (
	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/crypto"
)

type Authority [33]uint8

func (authority Authority) IntoEthereumAddress() (common.Address, error) {
	pub, err := crypto.DecompressPubkey(authority[:])
	if err != nil {
		return common.Address{}, err
	}
	address := crypto.PubkeyToAddress(*pub)
	if err != nil {
		return common.Address{}, err
	}

	return address, nil
}
