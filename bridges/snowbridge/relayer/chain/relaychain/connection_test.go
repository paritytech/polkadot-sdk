// Copyright 2020 Snowfork
// SPDX-License-Identifier: LGPL-3.0-only

package relaychain_test

import (
	"context"
	"testing"

	"github.com/snowfork/snowbridge/relayer/chain/relaychain"
)

func TestConnect(t *testing.T) {
	conn := relaychain.NewConnection("ws://127.0.0.1:9944/")
	err := conn.Connect(context.Background())
	if err != nil {
		t.Fatal(err)
	}

	conn.Close()
}
