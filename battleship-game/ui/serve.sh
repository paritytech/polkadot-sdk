#!/bin/bash
# Optional HTTP server for the Battleship UI
#
# NOTE: You don't need this server!
# The index.html file can be opened directly in any modern browser.
# Use this only if your browser has strict security settings.
#
# To open directly:
#   firefox dist/index.html
#   open dist/index.html (macOS)
#   start dist/index.html (Windows)

echo "============================================"
echo "  Battleship UI - Optional HTTP Server"
echo "============================================"
echo ""
echo "⚠️  NOTE: This server is OPTIONAL!"
echo "   You can open dist/index.html directly in your browser."
echo "   This server is only needed for browsers with strict security."
echo ""
echo "Starting HTTP server..."
echo "URL: http://localhost:8080"
echo ""
echo "Press Ctrl+C to stop"
echo "============================================"
echo ""

cd "$(dirname "$0")/dist"

if command -v python3 &> /dev/null; then
    python3 -m http.server 8080
elif command -v python &> /dev/null; then
    python -m SimpleHTTPServer 8080
else
    echo "Error: Python not found."
    echo ""
    echo "Instead, just open the file directly:"
    echo "  firefox $(pwd)/index.html"
    exit 1
fi
