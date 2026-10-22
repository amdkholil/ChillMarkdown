# AGENTS.md

Panduan untuk coding agent yang bekerja di repo ini.

## Ringkasan project
**chillMarkdown** — aplikasi desktop markdown viewer/editor berbasis **Tauri 2 (Rust) + Vue 3 + Vite**.
Fitur: navigasi internal (TOC kanan), mermaid, export PDF, split editor, light/dark mode, LaTeX, zoom.

## Struktur
```
index.html                  entry HTML
src/main.js                 bootstrap Vue
src/App.vue                 UI utama markdown viewer (semua fitur frontend di sini)
src-tauri/                  sisi Rust (Tauri 2)
  src/lib.rs                command Tauri + setup plugin
  Cargo.toml                deps Rust
  capabilities/default.json window permission
  tauri.conf.json           konfigurasi window/bundle
docs/plan.md                rencana arsitektur & keputusan desain
docs/tasks.md               daftar tugas (HARUS di-update setiap task selesai)
CHANGELOG.md                riwayat versi (Keep a Changelog)
SECURITY.md                 kebijakan keamanan
LICENSE                     Apache License 2.0
.github/workflows/ci.yml    CI lintas OS: bun install, vite build, cargo check, clippy (-D warnings), cargo build debug
.github/workflows/release.yml    build installer 3 OS via tauri-apps/tauri-action, dipicu push tag v* (draft release), butuh secrets TAURI_SIGNING_PRIVATE_KEY(_PASSWORD)
plugins: updater            auto updater: cek otomatis + tombol update di App.vue; endpoint latest.json GitHub Releases; keypair signer di ~/.tauri/chillmarkdown-updater.key (jangan pernah commit private key)
```

## Perintah
| Tujuan | Perintah |
|---|---|
| Dev (dengan window Tauri) | `bun run tauri dev` |
| Dev frontend saja (localhost:1420) | `bun run dev` |
| Build frontend | `bun run build` |
| Build installer | `bun run tauri build` |
| Cek Rust saja | `cargo check` di `src-tauri/` |
| Jadikan default viewer .md (Linux, user scope) | buat binary dulu (`cargo build --release` di src-tauri), lalu `scripts/install-default-viewer.sh [path_binary]` |
| Bump versi | sync ketiga manifest: `package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` (+ `cargo update -p chillmarkdown` untuk Cargo.lock) lalu tambah entri `CHANGELOG.md` |

## Dukungan lintas OS
Paket installer per-OS dibuat oleh `tauri build` di OS masing-masing (`.deb`/`.AppImage` Linux, `.msi`/`.exe` NSIS Windows, `.dmg`/`.app` macOS). Kode yang platform-specific dijaga dengan `#[cfg(...)]`:
- macOS: `RunEvent::Opened` (file dari Finder) + `RunEvent::Reopen` (klik dock) di `lib.rs`.
- Windows/Linux: argv via `tauri-plugin-single-instance`.
- Shortcut frontend menerima Ctrl **atau** Cmd (`hasPrimaryModifier` di `App.vue`).
- Jangan menambah dependency Rust/Frontend yang tidak support ketiga OS tanpa cek dulu. Rust command harus tidak memiliki asumsi path style (`/` vs `\` — di App.vue pakai regex `[/\\]`).

Bundler metapackage: **bun** (jangan gunakan npm/yarn/pnpm).

## Konvensi
- Vue 3 `<script setup>` (Composition API, bahasa JS, bukan TS).
- Rust edition 2024.
- Tidak menggunakan commit kecuali diminta.
- Jangan menambah komentar kecuali diminta.
- Update `docs/tasks.md` setiap kali sebuah task selesai (tandai `[x]` + catatan).

## Gotchas
- `vite.config.js` mengunci port **1420** (`strictPort: true`) — jangan diubah, Tauri mengandalkannya.
- Mermaid dan KaTeX harus re-render ketika tema (light/dark) berubah.
- PDF diekspor via `html2pdf.js` (browser library) lalu disimpan lewat command Rust `save_file_bytes` — jangan pakai window.print karena dialog simpan PDF tidak
mengikuti folder default Documents.
- State file aktif (`content`, `currentPath`, `dirty`) adalah **computed proxy** ke `activeTab` (array `tabs`, refs `activeTabId` — jangan dipake dengan `activeId` yang adalah scrollspy heading di TOC). Tambah per-tab state baru via `makeTab()`.
- Rust command yang memakai dialog blocking (`blocking_pick_file`/`blocking_save_file`/`blocking_show`) **wajib** `async fn` + `tauri::async_runtime::spawn_blocking` — kalau dipanggil langsung di main thread (sync command), event loop GTK macet dan app muncul dialog force close (terutama Linux).
- Semua dialog file (open/save/export) ada di sisi Rust: `show_open_dialog`, `show_save_dialog`, `documents_dir`.
- File dibuka dari OS masuk sebagai argv: ditangani `tauri-plugin-single-instance` + event `cm:open-file` + command `take_startup_file` (fallback supaya payload tidak hilang sebelum listener frontend siap).
- `preview` mengandalkan unit `em`, karena zoom diaplikasikan lewat `font-size` pada container preview.
