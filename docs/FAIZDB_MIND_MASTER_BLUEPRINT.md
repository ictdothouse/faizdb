# 📘 FAIZDB-MIND: BLUEPRINT INDUK & PELAN PELAKSANAAN KOGNITIF MULTI-PLATFORM

**Dokumen Rujukan Rasmi & Arahan Siap Bina (Ready-to-Execute Blueprint)**  
**Status:** Draf Akhir Diluluskan  
**Pengarang:** ICT HOUSE / FaizDB Project  

---

## 📌 Arahan Pantas untuk Pelaksanaan Masa Hadapan
> **Bila abang sudah bersedia untuk membina produk ini kelak, abang hanya perlu beri arahan ini kepada saya atau AI:**  
> *"Sila bina modul FaizDB-Mind Fasa [1/2/3/4] berpandukan pelan induk di docs/FAIZDB_MIND_MASTER_BLUEPRINT.md"*

---

## 1. Ringkasan Eksekutif & Falsafah Inovasi

**FaizDB-Mind** ialah sebuah enjin AI Kognitif dan Ingatan Pintar (*Self-Evolving Cognitive Engine*) yang direka khas untuk membolehkan **sesiapa sahaja yang hanya mempunyai komputer riba bajet, telefon pintar, atau pelayar web biasa** memiliki sistem AI peribadi yang sangat pintar, pantas, 100% luar talian (*offline*), dan selamat.

### 🌟 3 Tiang Inovasi Utama:
1. **Satu Binary Tanpa Dependensi (Zero-Dependency):** Menyatukan pangkalan data multi-model (**FaizDB**), enjin graf dan vektor (**VectorGraph**), serta enjin pemprosesan bahasa tempatan (**Local SLM Inference**) dalam satu fail ringan (~10MB–25MB binary).
2. **Kecil & Jimat Memori (<1.5 GB RAM):** Menggunakan algoritma kuantisasi 1-bit dan pemangkasan dinamik agar boleh berjalan pada komputer riba RM800 (4GB/8GB RAM) tanpa memerlukan sebarang kad grafik (GPU).
3. **Pembelajaran Sinaptik Hebbian (Evolusi Tanpa GPU):** AI belajar dan menjadi semakin pintar setiap hari mengikut corak fikiran pemiliknya melalui pengemaskinian topologi graf ingatan, tanpa melakukan *backpropagation* neural yang memanaskan komputer.

---

## 2. Matriks Pengedaran & Pemasangan Merentasi Semua Peranti (Multi-Platform Distribution)

Sistem ini direka untuk menyokong pelbagai kaedah pemasangan yang sangat mudah (*Plug & Play*) tanpa pengguna perlu tahu selok-belok teknikal:

```
                                  +-----------------------+
                                  |      FAIZDB-MIND      |
                                  +-----------------------+
                                              |
      +-------------------+-------------------+-------------------+-------------------+
      |                   |                   |                   |                   |
      v                   v                   v                   v                   v
[Desktop Native]   [One-Line Script]   [Package Managers]   [Static Website]     [Mobile & Edge]
  .exe / .dmg         curl | sh           npm / cargo        WASM + OPFS          Android APK
  (Double-Click)      irm | iex          docker run         (GitHub Pages)       Termux / IoT
```

### 2.1. Pemasangan Desktop Asli (Zero-Config Executable)
* **Windows:** Fail tunggal `faizdb-mind.exe` (Dwi-klik untuk terus jalankan).
* **macOS:** Fail `.dmg` atau binary universal Apple Silicon / Intel.
* **Linux:** Fail binary `faizdb-mind` (disertakan pakej `.deb` dan `.rpm`).

### 2.2. Skrip Pemasangan Satu Baris (Universal Terminal Install)
Pengguna hanya perlu salin dan tampal satu baris arahan:
* **Linux & macOS:**
  ```bash
  curl -fsSL https://faizdb.org/install.sh | sh
  ```
* **Windows (PowerShell):**
  ```powershell
  irm https://faizdb.org/install.ps1 | iex
  ```

### 2.3. Pengurus Pakej Popular Dunia
* **NPM / NPX (Komuniti Javascript/Node):**
  ```bash
  npx @faizdb/mind
  ```
  *(Pengguna tidak perlu pasang apa-apa, arahan ini terus memuat turun dan membuka Web UI dalam beberapa saat).*
* **Cargo (Komuniti Rust):**
  ```bash
  cargo install faizdb-mind
  ```
* **Docker Container (Pelayan / Homelab):**
  ```bash
  docker run -d -p 8080:8080 -v ~/faizdb-data:/data ictdothouse/faizdb-mind
  ```

---

## 3. Inovasi Luar Biasa: Bolehkah Berjalan di Website Statik?

👉 **JAWAPAN: YA, 100% BOLEH! DAN INI ADALAH SALAH SATU DAYA PENARIK TERHEBAT.**

### Bagaimana AI Boleh Berjalan di Laman Web Statik (Contoh: GitHub Pages / Vercel / Netlify)?
Laman web statik tidak mempunyai pelayan backend (hanya fail HTML, CSS, dan Javascript biasa dengan kos hosting **RM0 / Percuma Seumur Hidup**).

```
[ Pelayar Web Pengguna (Chrome / Safari / Edge / Mobile) ]
 +-----------------------------------------------------------------+
 | 1. Muat turun fail statik (HTML + JS + faizdb.wasm) [Sekali shj]|
 | 2. Enjin FaizDB WASM dihidupkan di dalam Web Worker             |
 | 3. WebGPU / Wasm-SIMD memproses Model Bahasa (Qwen-0.5B/1.5B)   |
 | 4. Semua data disimpan secara kekal dalam OPFS pengguna        |
 |    (Origin Private File System / IndexedDB)                     |
 +-----------------------------------------------------------------+
        ^                                        ^
        | Tiada data keluar ke internet          | Kos Hosting: RM0
        | 100% Privasi Tempatan                  | Skalabiliti: Infiniti
```

### Langkah Teknologinya:
1. **Enjin FaizDB WebAssembly (`faizdb-wasm`):**
   * Kod Rust FaizDB di-*compile* menjadi fail `faizdb.wasm` (~2MB).
   * Ia berjalan terus di dalam *Web Worker* pelayar web pengguna.
2. **Penyimpanan Kekal Tanpa Pelayan (OPFS - Origin Private File System):**
   * Pelayar web moden mempunyai cakera keras maya pantas sendiri (OPFS).
   * Nota, PDF, graf pengetahuan, dan vektor pengguna disimpan terus di dalam pemacu storan laptop/telefon mereka sendiri.
3. **Pemprosesan AI Berasaskan WebGPU / Wasm-SIMD:**
   * Model bahasa mini dimuat turun terus ke dalam cache pelayar web.
   * Apabila pengguna bersembang, cip telefon atau laptop mereka yang melakukan pengiraan.
4. **Kelebihan untuk Abang sebagai Pengasas:**
   * **Kos Pelayan: RM 0.00!** Walaupun ada 1,000,000 pengguna melawat laman web abang, abang tidak perlu membayar bil pelayan bernilai puluhan ribu ringgit, kerana setiap pengguna menggunakan perkakasan peranti mereka sendiri.
   * **PWA (Progressive Web App):** Pengguna boleh tekan butang *"Install App"* pada Google Chrome atau Safari telefon pintar mereka, dan laman web statik tersebut akan menjadi aplikasi telefon pintar penuh yang boleh dibuka secara luar talian (*offline*).

---

## 4. Algoritma Pemampatan & Pengoptimuman (Extreme Efficiency)

Untuk memastikan sistem berjalan lancar merentasi semua medium (termasuk laman web statik):

1. **Binary Quantization 1-Bit (BQ):**
   * Vektor dokumen dimampatkan sebanyak 97% (192 bait berbanding 6,144 bait).
   * Pengiraan jarak kosinus menggunakan arahan CPU `POPCNT` (1 kitaran jam CPU).
2. **BitNet b1.58 / Ternary Weights $\{-1, 0, 1\}$:**
   * Menukar operasi pendaraban titik apung kepada penambahan integer murni.
   * Mengurangkan haba CPU dan menjimatkan bateri laptop/telefon.
3. **KV-Cache Dynamic Dropping (H2O Stream):**
   * Memastikan perbualan yang panjang tidak membakar RAM. Memori dihadkan kepada <150MB sepanjang masa.
4. **Topological Graph Pruning:**
   * Mengurangkan saiz graf pengetahuan dengan menapis sisi-sisi hubungan yang tidak relevan secara automatik.

---

## 5. Enjin Pembelajaran Sinaptik (Self-Evolving Architecture)

AI ini berevolusi dan menjadi semakin bijak menggunakan 3 lapisan pembelajaran biologi:

```
[ Input Pengguna & Dokumen ]
            |
            v
[ FaizDB In-Memory VectorGraph ] <----+
            |                         |
            v                         | Maklum Balas Kognitif
[ Local Small LLM (1B) ]              | (Synaptic Plasticity)
            |                         |
            v                         |
[ Jawapan Tepat Kepada Pengguna ] ----+
            |
    (Semasa Laptop Idle)
            v
[ Sleep-Mode Consolidation Thread ]
  (Merumuskan fakta harian ke konsep teras)
```

1. **Hebbian Synaptic Weighting:** Sisi graf pengetahuan diperkukuh apabila pengguna mengesahkan jawapan, dan dilemahkan jika jawapan salah.
2. **Gold Memory Anchors:** Jawapan dan pembetulan penting dikunci sebagai "fakta emas" yang tidak akan berhalusinasi lagi.
3. **Konsolidasi Waktu Tidur (*Sleep-Mode*):** Bebenang latar belakang menyusun dan membersihkan memori episodik setiap kali komputer riba tidak digunakan.

---

## 6. Pelan Struktur Kod & Pakej (Crate Architecture)

Apabila projek ini mula dikodkan kelak, ia akan menggunakan struktur *workspace* Rust berikut:

```
faizdb/
├── faizdb-core/          # Enjin storan dokumen, LSM-Tree, & transaksi
├── faizdb-vector/        # Indeks HNSW, SIMD AVX2/NEON, & Binary Quantizer (1-bit)
├── faizdb-graph/         # GraphStore, PageRank, Dijkstra, & VectorGraph
├── faizdb-mind/          # [PRODUK BARU] Enjin orkestrasi kognitif & evolusi Hebbian
│   ├── src/
│   │   ├── brain.rs      # Pengurusan kitaran hidup memori & maklum balas sinaptik
│   │   ├── consolidation.rs # Thread latar belakang pemadatan memori waktu tidur
│   │   ├── quantizer.rs  # Kuantisasi vektor 1-bit & BitNet loader
│   │   └── server.rs     # Pelayan WebUI mikro in-memory (Axum / Actix)
│   └── Cargo.toml
├── faizdb-inference/     # Pengekodan model tempatan (Candle / Llama.cpp Rust FFI)
└── bindings/
    └── wasm/             # FaizDB WASM + OPFS untuk laman web statik & WebGPU
```

---

## 7. Fasa Pembangunan (Implementation Milestones)

| Fasa | Nama Fasa | Hasil Utama yang Akan Disiapkan |
| :---: | :--- | :--- |
| **Fasa 1** | **Enjin Inferens & Kuantisasi 1-Bit** | Modul `faizdb-inference` menggunakan `candle`/`llama-cpp-rs` dan sokongan `BinaryQuantizer` pada vektor. |
| **Fasa 2** | **Logik Evolusi Kognitif (Hebbian)** | Modul `faizdb-mind/src/brain.rs` untuk pengemaskinian graf dinamik berasaskan maklum balas pengguna. |
| **Fasa 3** | **Web UI Tempatan & Drag-and-Drop** | Antaramuka web mikro moden di `http://localhost:8080` untuk import PDF/nota secara terus ke graf. |
| **Fasa 4** | **Pakej Pengedaran Multi-Platform** | Kompilasi fail `.exe`, skrip `curl | sh`, pakej `npx @faizdb/mind`, dan versi laman web statik WASM. |

---

## 8. Pengesahan & Penutup
Dokumen ini mengunci keseluruhan seni bina, formula matematik, dan strategi produk bagi **FaizDB-Mind**. Semua hak intelek dan idea ini terpelihara di dalam repositori FaizDB untuk dibangunkan pada masa hadapan.
