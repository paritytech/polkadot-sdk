# Battleship Quick Start

Get the Battleship game running in 3 easy steps!

## Prerequisites

✅ Running blockchain network (relay chain + parachain with battleship pallet)
✅ Boot nodes configured in chain specs
✅ Modern web browser (Chrome/Firefox/Edge/Safari)
✅ Node.js 18+ (for bot only)

## Step 1: Open the Web UI (30 seconds)

**Option A: Direct Open (Recommended)** 🚀

```bash
# Just open the file!
cd battleship-game/ui/dist

# Double-click index.html in file manager
# OR use command line:
firefox index.html
```

**Option B: Use HTTP Server (if browser restricts file://)** 

```bash
cd battleship-game/ui
./serve.sh
# Open http://localhost:8080
```

🌐 The UI is a **31MB self-contained HTML file** - no server needed!

## Step 2: Start the Bot (1 minute)

```bash
cd battleship-game/bot

# If first time
npm install
npm run build

# Start the bot
npm start
```

🤖 Bot connects as Charlie and starts playing!

## Step 3: Play! (2 minutes)

**In the browser:**

1. Click "**Developer Mode**" toggle
2. Select **Alice**
3. Click "**Continue to Lobby**"
4. Click "**Create New Game**"
5. Set stake (e.g., **1 UNIT**)
6. Click "**Confirm & Create**"

**Wait for bot to join** (~10-30s for smoldot sync)

7. **Place your ships** on the left board
8. Click "**Commit Grid**"
9. **Battle begins!** Click enemy cells to attack

## What Happens

```
You (Alice)          Network           Bot (Charlie)
     │                  │                     │
     │──Create Game────>│                     │
     │                  │<────Scan games──────│
     │                  │──Bot joins─────────>│
     │<─Confirmed───────│                     │
     │                  │                     │
     │──Commit grid────>│                     │
     │                  │<────Commit grid─────│
     │                  │                     │
     │──Attack (3,5)───>│──Defend────────────>│
     │<─Result: MISS────│                     │
     │                  │<────Attack (2,7)────│
     │──Reveal─────────>│                     │
     │                  │──Result: HIT!──────>│
     │                  │                     │
     │    ...battle continues...              │
```

## No Server Needed! 🎉

The UI is a **single 31MB HTML file** with:
- ✅ All JavaScript (inline)
- ✅ Smoldot WASM (embedded)
- ✅ Chain specs (embedded)
- ✅ All styles (inline)

Just **open the file** in any modern browser - that's it!

## Features

### Web UI
- 🎨 Beautiful gradient UI
- 🚢 Interactive ship placement
- 🎯 Click-to-attack gameplay
- 🔐 Merkle proof verification
- 📱 Responsive design
- 📂 **Single file, no server required**

### Bot
- 🤖 Autonomous gameplay
- 🧠 AI strategy (hunt + search)
- ⚡ Fast reactions
- 🎲 Random ship placement
- 🔄 Multi-game support

### Technology
- 🌐 Smoldot light clients
- 🔗 Direct p2p blockchain connection
- 🔒 Byzantine-resistant
- 📦 **31MB self-contained HTML**
- 🚀 Modern TypeScript/Vite

## Sharing the Game

Since the UI is a single file, you can:

📧 **Email it**
💾 **Put on USB drive**
☁️ **Upload to cloud storage**
🌍 **Host on GitHub Pages**
🔗 **Share via IPFS**
📱 **Send via messaging**

Anyone can download and open - **no installation required!**

## Tips

💡 **Multiple players**: Open multiple browser tabs (Alice/Bob)

💡 **Bot vs Bot**: Run multiple bot instances with different accounts

💡 **Fast games**: Use small stakes (0.001 UNIT)

💡 **Debug**: Check browser console (F12) and bot logs

💡 **First sync**: Takes 10-30s for smoldot to connect

💡 **Offline play**: Works offline after initial load (needs nodes though)

## Troubleshooting

❌ **"No games available"**
- Wait 30s for bot to create game
- Or create your own game

❌ **File won't open**
- Try different browser (Chrome/Firefox recommended)
- Or use `./serve.sh` instead
- Modern browsers (2021+) support direct file open

❌ **Bot not connecting**
- Check bot logs for errors
- Verify chain is running
- Wait for smoldot sync (10-30s)

❌ **Slow performance**
- First connection takes longer (sync)
- Subsequent actions are fast
- 31MB file takes 5-10s to load initially

## Alternative: Serve via HTTP

If direct file open doesn't work:

```bash
# Provided script
cd battleship-game/ui
./serve.sh

# Or Python
cd battleship-game/ui/dist
python3 -m http.server 8080

# Or Node.js
cd battleship-game/ui/dist
npx http-server -p 8080
```

## Next Steps

📚 **Detailed docs:**
- `ui/STANDALONE.md` - Standalone file usage
- `ui/README.md` - UI documentation
- `bot/README.md` - Bot documentation
- `SMOLDOT-SETUP.md` - Complete architecture

🔧 **Customize:**
- Bot account: `bot/src/accounts.ts`
- AI strategy: `bot/src/game.ts`
- Chain specs: `chainSpecs.ts`

🌐 **Deploy:**
- GitHub Pages (single file)
- IPFS (decentralized)
- Any static host

🎮 **Have fun battling on the blockchain!** ⚓️🎯

---

**TL;DR:**
1. Open `battleship-game/ui/dist/index.html` in browser
2. Run `cd battleship-game/bot && npm start`
3. Play! No server needed! 🚀
