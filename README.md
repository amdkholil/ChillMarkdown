# chillMarkdown

[![CI](https://github.com/amdkholil/ChillMarkdown/actions/workflows/ci.yml/badge.svg)](https://github.com/amdkholil/ChillMarkdown/actions/workflows/ci.yml)
[![Release](https://github.com/amdkholil/ChillMarkdown/actions/workflows/release.yml/badge.svg)](https://github.com/amdkholil/ChillMarkdown/actions/workflows/release.yml)

Markdown viewer & editor desktop untuk **Linux**, **Windows**, dan **macOS** — berbasis
[Tauri 2](https://v2.tauri.app) (Rust) + Vue 3 + Vite. Repo: <https://github.com/amdkholil/ChillMarkdown>.

## Fitur

- **Navigasi internal di panel kanan (TOC)** — heading otomatis diberi `id` unik, klik untuk smooth-scroll, scrollspy menandai posisi baca.
- **Mermaid** — code fence ` ```mermaid ` dirender sebagai diagram; tema ikut light/dark.
- **LaTeX (KaTeX)** — inline `$...$` dan block `$$...$$`.
- **Export PDF** — satu klik, dialog simpan default di folder *Documents*.
- **Editor split-view** — klik `Edit` untuk mode editor (textarea + preview live); lebar pane bisa di-drag (15 %–70 %), double-click divider reset; default tetap mode View.
- **File properties** — modal berisi metadata file (path, ukuran, created/modified) dan statistik dokumen (words, lines, code blocks, mermaid, reading time).
- **About dialog** — tombol `?` di kanan toggle dark/light: versi, link repo & issue, dan dukungan [Buy me a coffee](https://buymeacoffee.com/amdkholil).
- **Multi-file tabs** — buka banyak file sekaligus; switch klik, close via `×`/middle-click/klik-kanan kosong, `Ctrl/Cmd+T` tab baru, `Ctrl/Cmd+W` tutup tab, `Ctrl/Cmd+Alt+←/→` pindah tab.
- **Light & dark mode** — toggle di toolbar; preferensi tersimpan.
- **Zoom** — tombol +/−/reset, `Ctrl/Cmd + scroll`, `Ctrl/Cmd + ±/0` (50 %–300 %).
- **Buka/simpan file** `.md` / `.markdown` / `.txt`, shortcut `Ctrl/Cmd + S`.
- **Default viewer** — bisa didaftarkan sebagai aplikasi pembuka `.md` di ketiga OS (lihat di bawah).
- **Auto updater** — app memeriksa update otomatis saat startup (5 detik, silent); jika ada versi baru, tombol `Update vX` muncul di toolbar — klik untuk unduh, install, dan app restart. Manual: `Ctrl/Cmd + U`.
- Autosave draft ke localStorage saat tanpa file.

## Build & Jalankan

Prasyarat: [bun](https://bun.sh), Rust (stable), dan dependensi webview per OS:

| OS | Dependensi build |
|---|---|
| Linux (Debian/Ubuntu) | `libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev` |
| Windows | WebView2 (termasuk Win10/11), MSVC Build Tools |
| macOS | Xcode Command Line Tools |

```bash
bun install

# mode pengembangan (hot reload)
bun run tauri dev

# build installer (.deb/.AppImage | .msi/.exe | .dmg/.app)
bun run tauri build

# build frontend saja
bun run build
```

> Binary installer harus dibangun di OS masing-masing.

## Menjadikan default viewer untuk `.md`

- **Linux**
  ```bash
  cd src-tauri && cargo build --release && cd ..
  ./scripts/install-default-viewer.sh     # desktop entry + xdg-mime default
  ```
- **Windows / macOS**: install paket hasil `bun run tauri build`, lalu klik kanan file → *Open with* → pilih chillMarkdown → *Always*.
  Double-click file berikutnya langsung membuka di app (multi-open diarahkan ke window yang sudah terbuka).

## Shortcut

| Aksi | Shortcut |
|---|---|
| Simpan | `Ctrl/Cmd + S` |
| Tab baru / tutup tab | `Ctrl/Cmd + T` / `Ctrl/Cmd + W` |
| Pindah tab | `Ctrl/Cmd + Alt + ← / →` |
| Zoom in/out/reset | `Ctrl/Cmd + + / − / 0` atau `Ctrl/Cmd + scroll` |
| Cek update manual | `Ctrl/Cmd + U` |
| Masuk/keluar editor | tombol `Edit` / `Esc` |

## Rilis & auto update

Updater memakai `tauri-plugin-updater` dengan manifest `latest.json` dari GitHub Releases:

1. Update manifest: `bun x tauri signer generate -w ~/.tauri/chillmarkdown-updater.key` (public key sudah tertanam di `tauri.conf.json`).
2. Simpan isi file **private key** di secret GitHub repo: `TAURI_SIGNING_PRIVATE_KEY` dan `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (kosong bila tanpa password).
3. Rilis: `git tag v1.2.0 && git push origin v1.2.0` → workflow `release.yml` membangun installer 3 OS + `latest.json` (ditandatangani) sebagai **draft release**.
4. **Publish** draft release di GitHub — setelah itu app yang sudah terinstall akan menawarkan update otomatis.

> Catatan macOS: auto-update produksi requires `.app` yang di-sign (codesign/notarization) — Tauri updater di macOS menolak bundel unsigned. Rencanakan Apple Developer ID bila mau menyalurkan ke user Mac.

## Struktur

```
src/App.vue              UI utama (viewer, editor, TOC, toolbar)
src-tauri/               sisi Rust: command file I/O, dialog, file-open OS
docs/plan.md             rencana & keputusan desain
docs/tasks.md            daftar tugas
agents.md                panduan coding agent
CHANGELOG.md             riwayat versi
SECURITY.md              kebijakan & laporan keamanan
```

## Kontribusi

Baca `agents.md` untuk konvensi project. PR yang lolos CI welcome. Installer untuk
ketiga OS dibangun otomatis oleh workflow [release.yml](.github/workflows/release.yml)
pada push tag `v*` (draft release).

## Lisensi

Distribusikan di bawah [Apache License 2.0](./LICENSE).
