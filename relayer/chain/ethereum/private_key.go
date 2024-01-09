package ethereum

import (
	"fmt"
	"io/ioutil"
	"strings"

	"github.com/snowfork/snowbridge/relayer/crypto/secp256k1"
)

func ResolvePrivateKey(privateKey, privateKeyFile string) (*secp256k1.Keypair, error) {
	var cleanedKey string

	if privateKey == "" {
		if privateKeyFile == "" {
			return nil, fmt.Errorf("private key not supplied")
		}
		contentBytes, err := ioutil.ReadFile(privateKeyFile)
		if err != nil {
			return nil, fmt.Errorf("failed to load private key: %w", err)
		}
		cleanedKey = strings.TrimPrefix(strings.TrimSpace(string(contentBytes)), "0x")
	} else {
		cleanedKey = strings.TrimPrefix(privateKey, "0x")
	}

	keypair, err := secp256k1.NewKeypairFromString(cleanedKey)
	if err != nil {
		return nil, fmt.Errorf("failed to parse private key: %w", err)
	}

	return keypair, nil
}
