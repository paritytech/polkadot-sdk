# Opening the Battleship UI as a Standalone File

The built `dist/index.html` is a **31MB self-contained HTML file** with everything embedded:
- All JavaScript code (inline in a `<script>` tag)
- Smoldot WASM binary (base64 encoded)
- Chain specifications
- All styles

## Opening the File

### ✅ Recommended: Direct File Open (Modern Browsers)

**Most modern browsers support opening the file directly:**

```bash
# Linux
firefox dist/index.html
# or
google-chrome dist/index.html
# or
microsoft-edge dist/index.html

# macOS
open dist/index.html

# Windows
start dist/index.html
```

**Or simply:**
1. Navigate to `battleship-game/ui/dist/` in your file manager
2. Double-click `index.html`
3. Browser opens the file

### Browser Compatibility

| Browser | Works with file:// | Notes |
|---------|-------------------|-------|
| **Chrome 90+** | ✅ Yes | Full support |
| **Firefox 89+** | ✅ Yes | Full support |
| **Edge 90+** | ✅ Yes | Full support |
| **Safari 15+** | ✅ Yes | Full support |
| **Brave** | ✅ Yes | Full support |

### ⚠️ Potential Issues

If you encounter issues with `file://` protocol (rare with modern browsers):

1. **CORS/Module errors**: Some very strict security settings might block ES modules from `file://`

   **Solution**: Use the simple HTTP server instead:
   ```bash
   ./serve.sh
   ```

2. **Older browsers**: Browsers older than 2021 might have issues

   **Solution**: Update your browser or use the HTTP server

## Verifying It Works

When you open the file, you should see:
1. The "BATTLESHIP" title with gradient effect
2. "Developer Mode" toggle
3. No errors in browser console (press F12)
4. Smoldot initializing (check console logs)

If you see errors about "cannot import" or "CORS", use the HTTP server method instead.

## Alternative: Simple HTTP Server

If direct file access doesn't work (very rare):

```bash
# Method 1: Use the provided script
cd battleship-game/ui
./serve.sh

# Method 2: Python HTTP server
cd battleship-game/ui/dist
python3 -m http.server 8080

# Method 3: Node.js http-server (if installed)
cd battleship-game/ui/dist
npx http-server -p 8080
```

Then open: http://localhost:8080

## Sharing the File

Since it's a single file, you can:

✅ **Email it** (31MB)
✅ **Put on USB drive**
✅ **Upload to cloud storage** (Dropbox, Google Drive, etc.)
✅ **Host on GitHub Pages** (single file deployment)
✅ **Host on IPFS** (decentralized hosting)
✅ **Send via messaging apps** (if they allow large files)

Anyone can download and open it directly - no installation required!

## Deployment Options

### GitHub Pages
```bash
# In your repo
git checkout -b gh-pages
cp battleship-game/ui/dist/index.html .
git add index.html
git commit -m "Deploy battleship UI"
git push origin gh-pages
```
Access at: `https://yourusername.github.io/yourrepo/`

### IPFS
```bash
# Install IPFS, then:
ipfs add battleship-game/ui/dist/index.html
# Share the resulting CID
```

### Any Static Host
Just upload `index.html` to:
- Netlify (drag & drop)
- Vercel (deploy single file)
- Cloudflare Pages
- Amazon S3
- Any web server

## Technical Details

The file uses:
- **Inline JavaScript**: Everything in `<script type="module">` tag
- **Base64 WASM**: Smoldot WASM binary encoded as data URI
- **Inline styles**: All CSS in `<style>` tag
- **No external dependencies**: Zero external resources

This works because `vite-plugin-singlefile` bundles everything during build.

## File Size Breakdown

Total: 31MB
- Smoldot WASM: ~30MB (the light client binary)
- Application code: ~800KB
- Chain specs: ~200KB

The large size is due to embedding the entire smoldot WASM binary, which provides full blockchain light client functionality.

## Why This Matters

This approach means:
- ✅ No server required
- ✅ Works offline (after first load)
- ✅ Can't break due to external dependencies
- ✅ Survives web service shutdowns
- ✅ Truly portable

Perfect for:
- Demos and presentations
- Sharing with non-technical users
- Archival purposes
- Air-gapped environments (after downloading)
- Censorship resistance
