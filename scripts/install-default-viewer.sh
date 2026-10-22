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

# 1. Pasang ikon ke user icon theme
ICON_DIR="$HOME/.local/share/icons/hicolor/128x128/apps"
mkdir -p "$ICON_DIR"
if [[ -f "$REPO_DIR/src-tauri/icons/128x128.png" ]]; then
  cp "$REPO_DIR/src-tauri/icons/128x128.png" "$ICON_DIR/chillmarkdown.png"
fi

# 2. Buat .desktop file dengan %F agar terdaftar sebagai aplikasi pembuka file
DESKTOP_DIR="$HOME/.local/share/applications"
DESKTOP_FILE="$DESKTOP_DIR/$APP_ID.desktop"
mkdir -p "$DESKTOP_DIR"

cat > "$DESKTOP_FILE" <<EOF
[Desktop Entry]
Type=Application
Name=Chill Markdown
GenericName=Markdown Viewer & Editor
Comment=Calm markdown viewer and editor with live preview, mermaid, LaTeX, and PDF export
Exec=$BINARY %F
Icon=chillmarkdown
Terminal=false
StartupWMClass=chillmarkdown
Categories=Office;TextEditor;Utility;
Keywords=markdown;md;viewer;editor;chill;
MimeType=text/markdown;text/x-markdown;application/x-markdown;text/plain;
NoDisplay=false
EOF

# 3. Update database desktop dan mimeinfo cache user
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$DESKTOP_DIR"
fi

if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" 2>/dev/null || true
fi

# 4. Daftarkan Chill Markdown di [Added Associations] dan sebagai default untuk tipe markdown
MIME_TYPES=("text/markdown" "text/x-markdown" "application/x-markdown")

for mime in "${MIME_TYPES[@]}"; do
  # gio mime / xdg-mime mendaftarkan asosiasi ke mimeapps.list
  if command -v gio >/dev/null 2>&1; then
    gio mime "$mime" "$APP_ID.desktop" 2>/dev/null || true
  elif command -v xdg-mime >/dev/null 2>&1; then
    xdg-mime default "$APP_ID.desktop" "$mime" 2>/dev/null || true
  fi
done

echo "Terkirim: $DESKTOP_FILE"
echo "Ikon terpasang di: $ICON_DIR/chillmarkdown.png"
echo "Status asosiasi untuk text/markdown:"
if command -v gio >/dev/null 2>&1; then
  gio mime text/markdown
elif command -v xdg-mime >/dev/null 2>&1; then
  echo "Default: $(xdg-mime query default text/markdown)"
fi

