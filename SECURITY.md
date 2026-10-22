# Security Policy / Kebijakan Keamanan

## Versi yang didukung

| Versi | Didukung |
|---|---|
| 1.0.x | ✅ |
| < 1.0 | ❌ |

## Melaporkan kerentanan

Jika Anda menemukan kerentanan keamanan:

1. **Jangan buka issue publik.**
2. Laporkan via [GitHub Security Advisory](../../security/advisories/new) pada repo ini, atau email maintainer (set alamat email sebelum publikasi).
3. Sertakan: langkah reproduksi, versi app, OS + versi, dan (bila ada) PoC.

Target respons: konfirmasi awal dalam 7 hari, perbaikan rilis dalam 30 hari untuk kerentanan berdampak tinggi.

## Lingkup (scope) dan catatan desain

- **CSP dikonfigurasi `null`** (bawaan template) karena app membutuhkan CSS/JS inline dari bundel dan rendering HTML dari markdown. Konten markdown dirender sebagai HTML di webview — **jangan buka file markdown dari sumber tidak terpercaya** dalam mode `html: true` tanpa memahami risikonya (potensi injeksi markup/iframe).
- File markdown dapat menyematkan link eksternal; link `http(s)` dibuka **di browser eksternal** (plugin opener), bukan di dalam webview.
- Pemrosesan file hanya lokal: read/save markdown & PDF melalui std::fs Rust; tidak ada request jaringan, telemetri, atau upload.
- Dialog native (open/save) dijalankan dari sisi Rust; capability diberikan minimal (`core:default`, `opener:default`, `dialog:default`).
- PDF diekspor ke path yang dipilih user melalui dialog; app tidak pernah menulis di luar path tersebut (kecuali draft localStorage di webview profile).
- Mermaid dirender dengan `securityLevel` default (strict) — klik/href di dalam diagram tidak dieksekusi.
- **Auto updater**: hanya menerima paket yang ditandatangani dengan minisign key yang public-nya tertanam di `tauri.conf.json` (mismatch = ditolak); endpoint self-hosted GitHub Releases; private key **tidak** ada di repo (hanya sebagai GitHub Secret). Update di-install lalu app di-restart via `tauri-plugin-process`.

## Known limitations

- Ekspor PDF menghasilkan halaman raster (pratinjau berbasis html2canvas), bukan teks vektor — kualitas/ukuran file memengaruhi pengalaman, tetapi tidak ada isu keamanan.
- Content `html: true` markdown viewer: jika ingin membatasi, ubah `html: false` di `src/App.vue` (trade-off: HTML markdown tidak dirender).
