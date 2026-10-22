#!/usr/bin/env bash
# Menginstall Chill Markdown sebagai file association .md/.markdown di Linux (dev machine).
# Hanya untuk user saat ini (tanpa root).
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BINARY="${1:-$REPO_DIR/src-tauri/target/release/chillmarkdown}"
APP_ID="com.kholil.chillmarkdown"

if [[ ! -x "$BINARY" ]]; then
  echo "Binary tidak ditemukan: $BINARY"
  echo "Bangun dulu dengan: cd src-tauri && cargo build --release"
  exit 1
fi

DESKTOP_DIR="$HOME/.local/share/applications"
DESKTOP_FILE="$DESKTOP_DIR/$APP_ID.desktop"
mkdir -p "$DESKTOP_DIR"

cat > "$DESKTOP_FILE" <<EOF
[Desktop Entry]
Type=Application
Name=Chill Markdown
GenericName=Markdown Viewer
Comment=Markdown viewer dengan mermaid, LaTeX, dan export PDF
Exec=$BINARY %f
Terminal=false
Categories=Office;TextEditor;Viewer;
Keywords=markdown;md;viewer;editor;
MimeType=text/markdown;text/x-markdown;application/x-markdown;
NoDisplay=false
EOF

if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$DESKTOP_DIR"
fi

# set Chill Markdown sebagai default app untuk tipe markdown
if command -v xdg-mime >/dev/null 2>&1; then
  xdg-mime default "$APP_ID.desktop" text/markdown text/x-markdown application/x-markdown 2>/dev/null || true
fi

echo "Terkirim: $DESKTOP_FILE"
echo "Default viewer untuk text/markdown ->:"
xdg-mime query default text/markdown 2>/dev/null || true
