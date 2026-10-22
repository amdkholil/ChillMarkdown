# Changelog

Semua perubahan penting pada project ini didokumentasikan di file ini.

Format mengikuti [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
dan project menggunakan [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.1.0] - 2026-10-22

### Fixed

- Dialog native (Open/Save/Export/confirm) memblokir main thread GTK — app hang dan muncul dialog "force close" saat klik tombol Open di Linux. Command dialog kini `async` + `spawn_blocking` (pola resmi tauri-plugin-dialog).
- Garis split editor tidak lurus dengan kursor saat drag: `flex-basis` editor dihitung % terhadap lebar workspace total (termasuk TOC) sementara rasio mouse relatif `total − TOC`, plus offset lebar divider. Fixed dengan membungkus editor/divider/preview dalam `.split-area` (TOC di luar) dan editor memakai `width: calc(ratio% − 4px)` sehingga tepi editor pas di bawah kursor.

### Changed

- Toolbar: label nama file di sebelah kiri toggle dark/light dihapus — info nama/status tersimpan di tab aktif dan modal File properties, toolbar lebih bersih.
- Icon **Save as new file** diubah dari floppy+pencil (terlalu mirip Save) menjadi dokumen dengan tanda plus (file-plus) agar lebih mudah dibedakan; icon Save tetap floppy disk klasik.
- Modal **About** baru (tombol `?` di kanan toggle dark/light): versi aplikasi dibaca runtime via `getVersion()`, link Source code & Report an issue ke GitHub, tombol dukung **Buy me a coffee** (https://buymeacoffee.com/amdkholil), lisensi Apache-2.0, hint shortcut `Ctrl/Cmd+U` untuk cek update.

### Added

- **Auto updater** (`tauri-plugin-updater` + `tauri-plugin-process`):
  - Cek update otomatis (silent) 4 detik setelah startup, tombol `Update vX` di toolbar, shortcut `Ctrl/Cmd + U` untuk cek manual.
  - Update diunduh, di-install, lalu app restart sendiri; proteksi: perubahan belum disimpan dilarang update.
  - Endpoint: `latest.json` dari GitHub Releases; artefak ditandatangani minisign (`bundle.createUpdaterArtifacts`), public key tertanam di `tauri.conf.json`.
  - `release.yml` membutuhkan secrets `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
  - macOS: updater produksi memerlukan `.app` yang di-codesign.
- UI berbahasa Inggris; tombol toolbar diganti icon SVG (tooltip + aria-label tetap didokumentasikan).
- **File properties modal** (tombol іnfo di toolbar): nama/path/tipe/ukuran, created & modified (metadata via command `file_info`), plus statistik dokumen (words, characters, lines, headings, fenced code blocks, mermaid diagrams, estimasi reading time); ditutup via ×, klik backdrop, atau `Esc`.
- **Custom tooltip** via directive `v-tooltip` menggantikan `title` native: muncul ~300 ms (bukan beberapa detik), instan saat tombol di-focus via keyboard, posisi fixed di atas/bawah dengan clamp ke viewport, tema-aware, hilang saat klik/scroll, `aria-label` tetap ada untuk aksesibilitas.
- **Split editor resizable**: divider antar textarea dan preview bisa di-drag (Pointer Events — mouse/touch), clamp 15 %–70 % dengan memperhitungkan lebar panel TOC, persist ke localStorage `cm:split`, double-click reset ke default 38 %, dan bisa diatur via arrow keys saat divider di-focus (ada focus ring).
- **Multi-file tabs**: buka beberapa file sekaligus dalam tab strip di bawah toolbar — klik untuk switch, `× »` atau middle-click menutup tab (konfirmasi bila ada perubahan belum disimpan), tombol `+` membuat tab baru, `Ctrl/Cmd+T` tab baru, `Ctrl/Cmd+W` tutup tab, `Ctrl/Cmd+Alt+←/→` pindah tab. File yang sama tidak dibuka dua kali (di-activate + reload). State single-file (`content`, `currentPath`, `dirty`) direfactor menjadi proxy computed ke tab aktif.

## [Unreleased]

- *belum ada perubahan baru*

## [1.0.0] - 2026-10-22

### Added

- Markdown viewer dengan preview live (markdown-it + highlight.js untuk syntax highlighting).
- Panel navigasi internal di sebelah kanan: daftar heading (TOC), smooth-scroll, scrollspy.
- Render diagram **Mermaid** (lazy-load), tema mengikuti light/dark mode.
- Render **LaTeX** via KaTeX (inline `$...$`, block `$$...$$`).
- **Export PDF** via html2pdf.js; dialog simpan default folder Documents (`<nama-file>.pdf`).
- Mode **Edit** split-view (textarea kiri, preview kanan, live debounce); default View, muncul hanya saat tombol Edit diklik.
- **Light & dark mode** dengan persist ke localStorage; tema mermaid ikut di-refresh.
- **Zoom** 50 %-300 %: tombol +/−/reset, `Ctrl/Cmd + scroll`, `Ctrl/Cmd + ±/0`, persist ke localStorage.
- Buka/simpan file `.md` / `.markdown` / `.txt` lewat dialog native (tauri-plugin-dialog).
- Autosave draft ke localStorage (`cm:content`) saat tanpa file terbuka.
- Konfirmasi perubahan belum disimpan sebelum buka file baru.
- File association `.md` / `.markdown` (bundle.fileAssociations) — `.txt` sengaja tidak diasosiasikan.
- Dukungan **default viewer** di 3 OS:
  - Linux: `scripts/install-default-viewer.sh` (desktop entry + `xdg-mime`).
  - Windows: registry via installer NSIS/MSI.
  - macOS: `CFBundleDocumentTypes`; file dari Finder ditangani `RunEvent::Opened`, klik dock via `RunEvent::Reopen`.
- Single-instance (`tauri-plugin-single-instance`): file yang dibuka lagi saat app berjalan diteruskan ke window pertama (event `cm:open-file` + fallback command `take_startup_file`).
- Shortcut lintas OS: `Ctrl/Cmd + S` (simpan), `Ctrl/Cmd + ± / 0` (zoom), `Esc` (keluar editor).
- Lisensi project: **Apache License 2.0** (file `LICENSE`, metadata `Cargo.toml` & `package.json`).

### Fixed

- Interop CJS `@vscode/markdown-it-katex` (`exports.default`) — plugin gagal terpasang jika langsung `md.use(...)`.
- Heading id dedup (heading duplikat mendapat suffix `-1`, `-2`, dst.).

## [0.1.0] - 2026-10-22

### Added

- Scaffolding awal Tauri 2 + Vue 3 + Vite (template `create-tauri-app`).
