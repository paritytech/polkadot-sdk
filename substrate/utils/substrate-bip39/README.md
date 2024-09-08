# Substrate BIP39

This is a crate for deriving secret keys for Ristretto compressed Ed25519 (should be compatible with Ed25519 at this
time) from BIP39 phrases.

## Why?

The natural approach here would be to use the 64-byte seed generated from the BIP39 phrase, and use that to construct
the key. This approach, while reasonable and fairly straight forward to implement, also means we would have to inherit
all the characteristics of seed generation. Since we are breaking compatibility with both BIP32 and BIP44 anyway (which
we are free to do as we are no longer using the Secp256k1 curve), there is also no reason why we should adhere to BIP39
seed generation from the mnemonic.

BIP39 seed generation was designed to be compatible with user supplied brain wallet phrases as well as being extensible
to wallets providing their own dictionaries and checksum mechanism. Issues with those two points:

1. Brain wallets are a horrible idea, simply because humans are bad entropy generators. It's next to impossible to
   educate users on how to use that feature in a secure manner. The 2048 rounds of PBKDF2 is a mere inconvenience that
   offers no real protection against dictionary attacks for anyone equipped with modern consumer hardware. Brain wallets
   have given users false sense of security. _People have lost money_ this way and wallet providers today tend to stick
   to CSPRNG supplied dictionary phrases.

2. Providing own dictionaries felt into the _you ain't gonna need it_ anti-pattern category on day 1. Wallet providers
   (be it hardware or software) typically want their products to be compatible with other wallets so that users can
   migrate to their product without having to migrate all their assets.

To achieve the above phrases have to be precisely encoded in _The One True Canonical Encoding_, for which UTF-8 NFKD was
chosen. This is largely irrelevant (and even ignored) for English phrases, as they encode to basically just ASCII in
virtually every character encoding known to mankind, but immediately becomes a problem for dictionaries that do use
non-ASCII characters. Even if the right encoding is used and implemented correctly, there are still [other caveats
present for some non-english dictionaries](https://github.com/bitcoin/bips/blob/master/bip-0039/bip-0039-wordlists.md),
such as normalizing spaces to a canonical form, or making some latin based characters equivalent to their base in
dictionary lookups (eg. Spanish `ñ` and `n` are meant to be interchangeable). Thinking about all of this gives me a
headache, and opens doors for disagreements between buggy implementations, breaking compatibility.

BIP39 does already provide a form of the mnemonic that is free from all of these issues: the entropy byte array. Since
verifying the checksum requires that we recover the entropy from which the phrase was generated, no extra work is
actually needed here. Wallet implementors can encode the dictionaries in whatever encoding they find convenient (as
long as they are the standard BIP39 dictionaries), no harm in using UTF-16 string primitives that Java and JavaScript
provide. Since the dictionary is fixed and known, and the checksum is done on the entropy itself, the exact character
encoding used becomes irrelevant, as are the precise codepoints and amount of whitespace around the words. It is thus
much harder to create a buggy implementation.

PBKDF2 was kept in place, along with the password. Using 24 words (with its 256 bits entropy) makes the extra hashing
redundant (if you could brute force 256 bit entropy, you can also just brute force secret keys), however some users
might be still using 12 word phrases from other applications. There is no good reason to prohibit users from recovering
their old wallets using 12 words that I can see, in which case the extra hashing does provide _some_ protection.
Passwords are also a feature that some power users find useful - particularly for creating a decoy address with a small
balance with empty password, while the funds proper are stored on an address that requires a password to be entered.

## Why not ditch BIP39 altogether?

Because there are hardware wallets that use a single phrase for the entire device, and operate multiple accounts on
multiple networks using that. A completely different wordlist would make their life much harder when it comes to
providing future Substrate support.
