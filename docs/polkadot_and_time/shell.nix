{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  buildInputs = [
    pkgs.marp-cli
    pkgs.chromium
  ];

  CHROME_PATH = "${pkgs.chromium}/bin/chromium";

  shellHook = ''
    echo "marp-cli available. Common commands:"
    echo "  marp slides.md -o slides.html          # html"
    echo "  marp slides.md -o slides.pdf --pdf     # pdf"
    echo "  marp slides.md -o slides.pptx --pptx   # powerpoint"
    echo "  marp -p slides.md                      # live preview"
  '';
}
