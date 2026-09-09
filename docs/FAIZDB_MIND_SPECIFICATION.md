# 🧠 FaizDB-Mind: Inovasi Enjin AI Kognitif Plug-and-Play untuk Peranti Biasa

**Sub-Projek Rasmi / Cabang R&D FaizDB**  
**Versi Dokumen:** 1.0 (Draf Seni Bina & Spesifikasi Algoritma)  
**Sasaran:** Komputer riba biasa tanpa GPU, peranti peribadi, dan edge computing.

---

## 1. Visi & Pengenalan

Bagi kebanyakan pengguna di seluruh dunia—pelajar, penyelidik bebas, peniaga kecil, dan pengaturcara yang tidak mempunyai bajet untuk membeli kad grafik mahal (NVIDIA RTX/H100) atau melanggan API berbayar—penggunaan AI sering terbantut oleh keperluan perkakasan yang tinggi.

**FaizDB-Mind** adalah cabang inovasi FaizDB yang bermatlamat untuk menyediakan:
1. **Plug & Play Sebenar:** Satu fail boleh laku tunggal (*single binary executable*) tanpa memerlukan Python, Docker, atau pemasangan perisian rumit.
2. **Ultra-Ringan & Boleh Jalan di CPU Laptop Biasa:** Menggunakan algoritma pemampatan radikal supaya keseluruhan sistem (Database + Embedding + Model Bahasa) hanya menggunakan **bawah 1.5 GB RAM**.
3. **Enjin GraphRAG Bersepadu:** Menggunakan enjin `VectorGraph` FaizDB sedia ada untuk carian ingatan pantas sub-2 milisaat.
4. **Fungsi "Train" & Evolusi Tanpa GPU:** Mekanisme pembelajaran berterusan (*continual learning*) berasaskan memori sinaptik dan konsolidasi graf—membolehkan AI menjadi semakin bijak setiap hari tanpa perlu melatih semula bobot neural (*backpropagation*) yang membakar laptop.

---

## 2. Algoritma Pemampatan & Pengurangan Beban (Extreme Lightweight Algorithms)

Bagi memastikan enjin ini boleh berjalan lancar di komputer biasa, 4 algoritma utama disepadukan:

```
+-------------------------------------------------------------------------+
|                          FaizDB-Mind Core Engine                        |
|                                                                         |
|  +---------------------------+       +-------------------------------+  |
|  |     Vector Compression    |       |        Graph Topology         |  |
|  | 1-Bit Binary Quantization | <---> |   Hebbian Dynamic Synapses    |  |
|  | (32x smaller, POPCNT CPU) |       | (Self-tuning edge weights)    |  |
|  +---------------------------+       +-------------------------------+  |
|               ^                                      ^                  |
|               |                                      |                  |
|  +---------------------------+       +-------------------------------+  |
|  |     Model Quantization    |       |      In-Memory KV Cache       |  |
|  |  BitNet b1.58 / GGUF Q4   | <---> |   StreamingLLM / H2O Drop     |  |
|  | (Integer Additions Only)  |       | (Zero memory explosion in chat|  |
|  +---------------------------+       +-------------------------------+  |
+-------------------------------------------------------------------------+
```

### 2.1. 1-Bit Binary Quantization (BQ) & Product Quantization (PQ)
* **Masalah:** Vektor standard ($f32$, 1536 dimensi) mengambil 6,144 bait bagi setiap nod. 100,000 dokumen akan memakan ~600MB RAM hanya untuk nombor perpuluhan.
* **Penyelesaian Algoritma:** 
  * Menukar setiap dimensi kepada 1 bit: jika nilai $> 0 \rightarrow 1$, jika $\le 0 \rightarrow 0$.
  * Saiz vektor berkurang dari 6,144 bait kepada **192 bait (penjimatan 32 kali ganda / 97%)**.
  * Pengiraan jarak kosinus digantikan dengan **Hamming Distance** menggunakan arahan perkakasan CPU satu kitaran: `_mm_popcnt_u64`.
  * **Hasil:** Carian vektor jutaan dokumen di laptop murah berlaku dalam masa kurang 0.5 milisaat.

### 2.2. BitNet b1.58 & Kuantisasi Ternari $\{-1, 0, 1\}$
* **Masalah:** Model bahasa standard melakukan pendaraban matriks titik apung (*floating point multiplications*) yang sangat membebankan CPU biasa.
* **Penyelesaian Algoritma:**
  * Menggunakan seni bina kuantisasi 1.58-bit di mana setiap bobot model hanyalah salah satu daripada tiga nilai: $-1$, $0$, atau $+1$.
  * Pendaraban matriks ($W \times X$) dihapuskan dan digantikan dengan **penambahan/penolakan integer mudah** (*pure integer addition*).
  * **Hasil:** Kelajuan pemprosesan meningkat 2x–3x ganda di CPU laptop tanpa GPU dan suhu pemproses kekal sejuk.

### 2.3. Pemampatan KV-Cache Dinamik (StreamingLLM / H2O Cache)
* **Masalah:** Semasa perbualan panjang dengan pengguna, memori *Key-Value Cache* membesar tanpa had dan menyebabkan laptop kehabisan RAM (*Out of Memory*).
* **Penyelesaian Algoritma:**
  * Algoritma *Heavy Hitter Oracle (H2O)* mengekalkan 4 token pembukaan (*attention sinks*) dan token paling relevan sahaja, sambil membuang token perantaraan yang tidak aktif.
  * **Hasil:** Penggunaan RAM semasa bersembang dihadkan kepada siling tetap (bawah 150MB), membolehkan dialog tanpa henti.

### 2.4. Pemangkasan Graf Topologi Semantik (Graph Transitive Reduction)
* **Masalah:** Penambahan ribuan dokumen boleh menyebabkan nod graf terlalu padat dengan sambungan tidak penting (*noisy edges*).
* **Penyelesaian Algoritma:** Algoritma pemangkasan transisi (*transitive edge pruning*) membuang sisi sekunder jika wujud laluan terpendek primer yang lebih berautoriti.

---

## 3. Seni Bina Plug & Play: Zero Configuration

Pengguna sasaran tidak perlu menjadi jurutera perisian untuk menggunakan FaizDB-Mind:

1. **Satu Fail Sahaja (`faizdb-mind.exe`):**
   * Tiada pemasangan Python, tiada dependensi C++ compiler, tiada Docker.
2. **Penyediaan Automatik (Auto-Bootstrap):**
   * Apabila pengguna pertama kali menjalankan fail tersebut, sistem akan memuat turun secara automatik satu model bahasa ringan (contohnya: `Qwen-2.5-0.5B-GGUF` atau `Llama-3.2-1B-Q4`) dan model penukaran embedding mini (`all-MiniLM-L6-v2`, ~80MB).
3. **Antaramuka Web Tempatan Serta-Merta (Instant Local Web UI):**
   * Pelayan web mikro di dalam binary terus membuka pelayar ke `http://localhost:8080`.
   * Papan pemuka yang ringkas dan moden membolehkan pengguna **menyeret dan melepaskan fail (*drag-and-drop*)** seperti PDF, TXT, Markdown, atau kod pengaturcaraan.
   * FaizDB secara automatik memecahkan teks (*chunking*), mengekstrak entiti graf, dan membina indeks vektor.

---

## 4. Mekanisme "Train" & Model Evolve Tanpa GPU

Bagaimanakah sistem ini boleh "melatih" AI dan menjadikannya semakin hebat dari semasa ke semasa tanpa membakar perkakasan laptop?

```
             +---------------------------------------------+
             |                 PENGGUNA                    |
             +---------------------------------------------+
                               |         ^
             Tanya Soalan /    |         |  Jawapan Tepat
             Beri Dokumen      |         |  (0% Halusinasi)
                               v         |
             +---------------------------------------------+
             |            FaizDB-Mind Pipeline             |
             +---------------------------------------------+
                               |
               +---------------+---------------+
               |                               |
               v                               v
     +-------------------+           +-------------------+
     | VectorGraph Memory|           | Small LLM (1B)    |
     | (1.96ms In-Memory)|           | (Reasoning Engine)|
     +-------------------+           +-------------------+
               |                               ^
               +------ Konteks Terpilih -------+
                               |
                   Maklum Balas (Feedback Loop)
                               |
                               v
            +------------------------------------+
            |      Enjin Evolusi Kognitif        |
            | - Hebbian Synaptic Weighting       |
            | - Gold Memories Cache              |
            | - Sleep-Mode Memory Consolidation  |
            +------------------------------------+
```

### 4.1. Pembelajaran Sinaptik Hebbian (Hebbian Graph Reweighting)
* Berpandukan prinsip biologi neurosains: *"Neurons that fire together, wire together."*
* Apabila pengguna menyukai jawapan yang dijana, atau apabila sesuatu fakta kerap dirujuk dalam sesi perbualan, berat sambungan graf ($w_{ij}$) antara konsep-konsep tersebut ditingkatkan secara dinamik:
  $$w_{ij}^{(t+1)} = w_{ij}^{(t)} + \eta \cdot (\text{relevance\_feedback})$$
* Jika sesuatu fakta ditanda salah oleh pengguna, graf akan melemahkan sambungan tersebut serta-merta.
* **Kesan:** Graf pengetahuan FaizDB menyesuaikan diri secara automatik dengan corak pemikiran pengguna tanpa perlu sebarang pengiraan kecerunan (*gradient descent*).

### 4.2. Pangkalan Memori Keutamaan (Gold Memory Pairs)
* Apabila pengguna membetulkan jawapan AI (*"Bukan begitu, hakikat sebenarnya adalah ABC"*), sistem mencipta *Gold Anchor Node* dalam FaizDB.
* Pada masa hadapan, jika ada soalan yang mempunyai persamaan semantik tinggi dengan *Gold Anchor*, jawapan tepat yang telah disahkan pengguna akan digunakan secara terus.

### 4.3. Konsolidasi Memori Waktu Tidur (Sleep-Mode Consolidation)
* Apabila laptop berada dalam keadaan terbiar (*idle*) atau semasa dicas:
  * Bebenang latar belakang (*background thread*) membaca interaksi harian.
  * Ia merumuskan perbincangan harian menjadi konsep umum peringkat tinggi (*macro-concepts*).
  * Ini membolehkan AI "mengingati" intipati penting dan membuang butiran remeh yang membazirkan ruang memori.

---

## 5. Ringkasan Penggunaan Sumber di Laptop Biasa

| Komponen | Sumber yang Digunakan | Cara Dioptimumkan |
| :--- | :---: | :--- |
| **Enjin Teras FaizDB** | ~25 MB RAM | Struktur memori Rust tanpa pemungut sampah (*zero GC*). |
| **Indeks Vektor (BQ)** | ~15 MB RAM (100k nod) | 1-bit quantization dengan arahan CPU SIMD. |
| **Enjin Graf** | ~10 MB RAM | Senarai bersebelahan (*adjacency list*) berindeks padat. |
| **Model Embedding Mini** | ~90 MB RAM | Model kerdil ONNX runtime INT8. |
| **Model Bahasa (1B Q4)** | ~900 MB – 1.2 GB RAM | Format GGUF terkuantisasi 4-bit atau BitNet. |
| **JUMLAH SISTEM** | **~1.1 GB – 1.4 GB RAM** | **Sesuai untuk laptop 4GB / 8GB RAM biasa!** |

---

## 6. Pelan Tindakan Pelaksanaan (Roadmap Cabang FaizDB-Mind)

* [ ] **Fasa 1: Pengikatan Rust-Candle / GGUF Runner**
  * Membina modul `faizdb-inference` menggunakan pustaka Rust asli seperti `candle` atau `llama-cpp-rs`.
* [ ] **Fasa 2: Enjin Kuantisasi Vektor 1-Bit (Binary HNSW)**
  * Menambah fungsi `BinaryQuantizer` pada `faizdb-vector` untuk menjimatkan memori sebanyak 97%.
* [ ] **Fasa 3: Mekanisme Pembelajaran Hebbian pada `VectorGraph`**
  * Melaksanakan fungsi pengemaskinian berat sisi secara automatik berasaskan maklum balas pengguna.
* [ ] **Fasa 4: CLI & Papan Pemuka PWA Web Tempatan**
  * Membina pakej boleh laku kendiri (`faizdb-mind-standalone`) dengan antaramuka ringkas dan fungsi heret-dan-lepas dokumen.
