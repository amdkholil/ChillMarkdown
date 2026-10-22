# Chill Markdown — Rencana Aplikasi

Aplikasi desktop markdown viewer/editor berbasis **Tauri 2 + Vue 3** dengan fitur:

| Fitur | Implementasi |
|---|---|
| Navigasi internal (kanan) | Heading diberi `id` via custom renderer rule markdown-it (slugify unik). Panel kanan berisi TOC dari `md.parse()`, klik -> `scrollIntoView`, scrollspy via IntersectionObserver. |
| Mermaid | Code fence ` ```mermaid ` dirender khusus oleh highlight hook markdown-it (di-pass-through dengan data attribute), kemudian di-render async oleh `mermaid.render()` setelah HTML preview ter-update. Tema mengikuti mode light/dark (re-render saat ganti tema). |
| Export PDF | `html2pdf.js` (html2canvas + jsPDF) merender konten preview ke PDF di sisi webview. Dialog lokasi/nama file via Rust (`show_save_dialog`), default path = folder **Documents** + `<nama-file>.md.pdf`. |
| Edit | Toggle tombol "Edit". Mode edit = split horizontal: textarea kiri + preview kanan (live, debounce). Default tetap **View**. |
| Light & Dark mode | CSS variables pada `[data-theme="light"|"dark"]`, toggle di toolbar, persist ke localStorage. Mermaid theme ikut di-refresh. |
| LaTeX | `@vscode/markdown-it-katex` + CSS KaTeX. Inline `$...$`, block `$$...$$`. |
| Zoom | Tombol +/-/reset dan `Ctrl+Wheel`. Renderer preview pakai `em` sehingga cukup scale `font-size` root preview. Persist localStorage. |
| Buka/Simpan file | Rust commands + `tauri-plugin-dialog`: open/save .md, autosave ke path terakhir. |
| Zoom persisted | `localStorage` key `cm:zoom`. |

## Keputusan desain (final)
- Semua dialog (open/save/export) dilakukan di **Rust** (`blocking_pick_file`/`blocking_save_file`) supaya tidak perlu capability `fs` dan path tracing konsisten.
- File I/O via std::fs (`read_file`, `save_file`, `save_file_bytes` untuk PDF).
- Default path dialog save PDF = `app.path().document_dir()` (folder Documents) + nama file `<nama>.pdf`.
- PDF hasil html2pdf.js di-transfer sebagai byte array lewat command `save_file_bytes`.
- Konfirmasi unsaved-changes lewat command `confirm_discard` (dialog plugin, buttons OkCancelCustom).
- `@vscode/markdown-it-katex` adalah CJS dengan `exports.default` — perlu chain interop `mod.default?.default ?? mod.default ?? mod`.
- Konten prose disimpan di localStorage (`cm:content`, `cm:theme`, `cm:zoom`) sebagai draft saat tanpa file.

## Dukungan lintas OS (Linux/Windows/macOS)
- Semua fitur berjalan di webview, jadi platform-agnostic. Perhatian khusus:
  - **Linux**: default viewer diubah via `scripts/install-default-viewer.sh` (desktop entry + `xdg-mime`). Butuh `libwebkit2gtk-4.1-dev` untuk build.
  - **Windows**: association `.md` didaftarkan oleh installer NSIS/MSI (dibuat `tauri build`). WebView2 runtime di-install otomatis oleh bootstrapper. Shortcut pakai Ctrl; `windows_subsystem=windows` sudah diset.
  - **macOS**: file dibuka via Finder/LaunchServices ditangani `RunEvent::Opened { urls }` (Apple Event `odoc`, bukan argv) dan klik dock via `RunEvent::Reopen`. Shortcut zoom/save juga menerima Cmd (metaKey). File bundel `.md`/`.markdown` masuk CFBundleDocumentTypes dari `bundle.fileAssociations`.
  - `tauri-plugin-single-instance` mensupport ketiga OS (mutex Windows / D-Bus Linux / socket Unix macOS) — instance baru selalu meneruskan file ke instance pertama.
- Icon: `icon.icns` (macOS), `icon.ico` + Square logo (Windows Store), PNG (Linux) sudah didaftarkan di `bundle.icon`.
- Build binary hanya bisa dilakukan di OS masing-masing (atau CI matrix); `cargo check --target` lintas OS tidak tersedia di mesin ini.

## Default viewer untuk .md
- `tauri.conf.json` -> `bundle.fileAssociations` mendaftarkan `.md` / `.markdown` (MimeType `text/markdown`) saat paket terinstall (`NSIS`/`MSI` Windows, `CFBundleDocumentTypes` di macOS, `.deb`/`rpm`/AppImage buat desktop entry di Linux).
- File yang dibuka OS masuk sebagai argv (Windows/Linux): `tauri-plugin-single-instance` meneruskan argv ke window yang sudah jalan; instance pertama membaca argv di `setup`.
- macOS jalur berbeda: LaunchServices mengirim Apple Event, bukan argv — ditangani `RunEvent::Opened { urls }` (bukan setup).
- Frontend menerima file baru via event `cm:open-file` (fallback command `take_startup_file` supaya tak hilang jika event datang sebelum listener siap).
- Set default aktif di Linux (developer machine): `scripts/install-default-viewer.sh [path_binary]` — buat desktop entry di `~/.local/share/applications`, lalu `xdg-mime default` untuk `text/markdown`.
- Windows/macOS set default via OS: klik kanan file -> Open with -> set Chill Markdown sebagai default (atau Default Apps di Settings).

## Auto updater
- Plugin rust `tauri-plugin-updater` + `tauri-plugin-process` (relaunch), capability `updater:default` + `process:allow-restart`.
- Config `tauri.conf.json`: `bundle.createUpdaterArtifacts: true`, `plugins.updater` (aktif, dialog harus false karena UI custom, pubkey minisign, endpoint GitHub `releases/latest/download/latest.json`).
- Frontend `App.vue`: `check()` silent 4 detik setelah mount (hanya saat isTauri); update tersedia -> tombol `Update vX` di toolbar; klik -> `downloadAndInstall()` lalu `relaunch()`. Shortcut `Ctrl/Cmd + U` untuk cek manual. Update diblokir kalau masih ada perubahan belum disimpan.
- Signing: keypair di `~/.tauri/chillmarkdown-updater.key(.pub)`; private key diisi ke GitHub Secrets `TAURI_SIGNING_PRIVATE_KEY`/`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`; `tauri-action` membuat & menandatangani `latest.json` otomatis. Draft release harus di-publish manual agar endpoint `latest` terisi.
- macOS produksi: bundel harus di-codesign; updater menolak `.app` unsigned.
- Dev mode: updater tidak berfungsi di `tauri dev` (endpoint & signature hanya relevan pada bundel rilis) — fail silent.
