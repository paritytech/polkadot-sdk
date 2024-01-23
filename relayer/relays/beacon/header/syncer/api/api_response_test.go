package api

import (
	"encoding/json"
	"github.com/stretchr/testify/assert"
	"os"
	"testing"
)

func TestUnmarshalBlockResponse(t *testing.T) {
	blockResponse := BeaconBlockResponse{}
	data, _ := os.ReadFile("web/packages/test/testdata/beacon_block_6815804.json")
	if data != nil {
		err := json.Unmarshal(data, &blockResponse)
		assert.Nil(t, err)
	}
}
