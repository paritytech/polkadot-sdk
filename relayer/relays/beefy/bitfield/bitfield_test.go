package bitfield

import (
	"fmt"
	"math/big"
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestBitfieldMembers(t *testing.T) {
	x := big.NewInt(1)
	y := big.NewInt(1)
	z := big.NewInt(1)

	u := New([]*big.Int{x, y, z})
	fmt.Printf("%v\n", u)
	fmt.Printf("%v\n", u.Members())

	assert.Equal(t, u.Members(), []uint64{0, 256, 512})
}

func TestBitfieldMembers2(t *testing.T) {
	foo := make([]byte, 32)
	foo[0] = 128
	foo[31] = 1

	x := big.NewInt(1)
	x.SetBytes(foo)

	u := New([]*big.Int{x})
	fmt.Printf("%v\n", u)
	fmt.Printf("%v\n", u.Members())

	assert.Equal(t, u.Members(), []uint64{0, 255})
}

func TestBitfiledMembers3(t *testing.T) {
	var x, y, z, w big.Int

	// Four uint256 with first and last bit set
	x.SetString("8000000000000000000000000000000000000000000000000000000000000001", 16)
	y.SetString("8000000000000000000000000000000000000000000000000000000000000001", 16)
	z.SetString("8000000000000000000000000000000000000000000000000000000000000001", 16)
	w.SetString("8000000000000000000000000000000000000000000000000000000000000001", 16)

	u := New([]*big.Int{&x, &y, &z, &w})
	fmt.Printf("%v\n", u)
	fmt.Printf("%v\n", u.Members())

	assert.Equal(t, u.Members(), []uint64{
		/* x */ 0, 255,
		/* y */ 256, 511,
		/* z */ 512, 767,
		/* w */ 768, 1023,
	})
}
