package parachain

import (
	"fmt"
	"io/ioutil"
	"log"
	"strings"

	"github.com/snowfork/snowbridge/relayer/crypto/sr25519"
)

func ResolvePrivateKey(privateKey, privateKeyFile string) (*sr25519.Keypair, error) {
	var cleanedKeyURI string

	if privateKey == "" {
		if privateKeyFile == "" {
			return nil, fmt.Errorf("private key URI not supplied")
		}
		content, err := ioutil.ReadFile(privateKeyFile)
		if err != nil {
			log.Fatal(err)
		}
		cleanedKeyURI = strings.TrimSpace(string(content))
	} else {
		cleanedKeyURI = privateKey
	}

	keypair, err := sr25519.NewKeypairFromSeed(cleanedKeyURI, 42)
	if err != nil {
		return nil, fmt.Errorf("unable to parse private key URI: %w", err)
	}

	return keypair, nil
}
