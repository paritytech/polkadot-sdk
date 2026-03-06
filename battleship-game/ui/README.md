# Battleship UI

A web-based UI for playing the Battleship game on the blockchain using a smoldot light client.

## 🚀 Quick Start - No Server Required!

The UI is built as a **single self-contained HTML file** (31MB) that includes everything:
- All JavaScript code (inline)
- Smoldot WASM binary (embedded)
- Chain specifications
- Styles

**Just open the file directly in your browser:**

```bash
# Double-click in file manager, or:
firefox battleship-game/ui/dist/index.html

# macOS
open dist/index.html

# Windows
start dist/index.html
```

No installation, no server, no dependencies! ✨

### Optional: Use HTTP Server

If your browser has strict security settings (rare), use the included server:

```bash
./serve.sh
# Open http://localhost:8080
```

## Building from Source

```bash
# Install dependencies
npm install

# Build the single HTML file
npm run build

# Output: dist/index.html (31MB)
```

## Development Mode

For development with hot reload:

```bash
npm run dev
# Open http://localhost:3000
```

## Features

### Light Client Architecture

The UI uses **smoldot** - a light client that:
- ✅ Runs entirely in the browser
- ✅ Connects directly to the p2p network (no RPC server needed)
- ✅ Verifies all data cryptographically
- ✅ Uses minimal resources compared to a full node

### Chain Specifications

The UI connects to:
- **Relay Chain**: Rococo local testnet
- **Parachain**: Battleship parachain (paraId: 2000)

Chain specs are embedded in the HTML file.

### Developer Mode

Toggle "Developer Mode" on the wallet screen:
1. Choose between Alice and Bob accounts
2. Open multiple browser tabs to play both sides
3. Perfect for local testing

### Wallet Support

In production mode, supports browser wallet extensions:
- Polkadot.js Extension
- Talisman
- SubWallet
- Any injected wallet provider

## Game Flow

1. **Account Selection**: Developer mode (Alice/Bob) or wallet extension
2. **Lobby**: Create a new game or join an existing waiting game
3. **Ship Placement**: Place your ships on your board
4. **Commit Grid**: Submit merkle root of your ship positions
5. **Battle**: Take turns attacking opponent's grid
6. **Reveal**: Reveal attacked cells with merkle proofs
7. **Victory**: Winner receives the pot!

## Deployment

Since it's a single file, you can:

✅ **Share directly**: Email, USB drive, cloud storage
✅ **GitHub Pages**: Upload to gh-pages branch
✅ **IPFS**: Decentralized hosting
✅ **Any static host**: Netlify, Vercel, S3, etc.

See [STANDALONE.md](STANDALONE.md) for detailed deployment instructions.

## Technical Architecture

### Build System

- **Vite**: Modern build tool with fast HMR
- **vite-plugin-singlefile**: Bundles everything into one HTML file
- **TypeScript**: Type-safe development

### Connection Flow

```
Browser
  └─> Smoldot Light Client (WASM, embedded in HTML)
       ├─> Relay Chain (Rococo local)
       └─> Parachain (Battleship)
            └─> Battleship Pallet
```

### File Structure

```
ui/
├── dist/
│   └── index.html           # 31MB self-contained file ⭐
├── src/
│   ├── main.ts              # Entry point
│   ├── chain/
│   │   ├── client.ts        # Smoldot client initialization
│   │   ├── chainSpecs.ts    # Relay & parachain specs
│   │   ├── accounts.ts      # Account management
│   │   └── battleship.ts    # Pallet interaction wrapper
│   ├── game/
│   │   ├── Board.ts         # Game board rendering
│   │   ├── OnchainGame.ts   # Game state management
│   │   └── ...
│   └── ui/
│       └── ...              # UI components
├── index.html               # Source HTML template
├── vite.config.ts           # Vite configuration (singlefile plugin)
├── serve.sh                 # Optional HTTP server
├── README.md                # This file
└── STANDALONE.md            # Detailed standalone usage guide
```

## Network Requirements

For the UI to work, you need:

1. **Running nodes** with the battleship pallet:
   - Relay chain node (Rococo local)
   - Parachain collator (Battleship)

2. **Boot nodes** must match those in the embedded chain specs:
   - Relay: `/ip4/127.0.0.1/tcp/35439/ws/p2p/12D3KooW...`
   - Parachain: `/ip4/127.0.0.1/tcp/44071/ws/p2p/12D3KooW...`

3. **Network discovery**: Smoldot connects to boot nodes and discovers peers

## Browser Compatibility

Works in all modern browsers (2021+):
- Chrome/Edge 90+
- Firefox 89+
- Safari 15+
- Brave

Requires:
- WebAssembly support
- ES modules support
- IndexedDB (for smoldot storage)

## File Size

The 31MB size includes:
- **~30MB**: Smoldot WASM binary (the entire light client)
- **~800KB**: Application code
- **~200KB**: Chain specifications

The large size enables **zero-dependency** operation - everything needed to connect to the blockchain is embedded.

## Troubleshooting

### "Cannot open file" errors
- Modern browsers should open it directly
- If issues, use `./serve.sh` instead

### Smoldot connection issues
- Check that relay and parachain nodes are running
- Verify boot nodes match your running nodes
- Check browser console (F12) for errors
- Wait 10-30s for initial sync

### Long load time
- First load: ~5-10 seconds (loading 31MB)
- Browser caches after first load
- Subsequent loads are faster

## Development

### Hot Reload

```bash
npm run dev
```

Changes to TypeScript/HTML are instantly reflected.

### Debug Mode

Open browser DevTools (F12) to see:
- Smoldot log messages
- PAPI connection status
- Game state changes
- Transaction submissions

### Testing

```bash
npm test  # Runs Playwright tests
```

## Why Single File?

**Advantages:**
- ✅ Works without a server
- ✅ Can't break due to external dependencies
- ✅ Survives web service shutdowns
- ✅ Easy to share and archive
- ✅ Works offline (after first load)
- ✅ Truly portable

**Trade-offs:**
- ❌ Large file size (31MB)
- ❌ No code splitting
- ❌ Longer initial load

For a decentralized blockchain application, these trade-offs are worth it - users get a truly self-contained application that can't be censored or taken down.

## License

Part of the Polkadot SDK battleship example.
