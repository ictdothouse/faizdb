# 🎮 FaizDB: Panduan Senario Penggunaan & Senibina Penyelesaian (Use Cases & Solutions)

Dokumen ini memperincikan senario dunia sebenar di mana **FaizDB** menjadi pilihan utama berbanding pangkalan data tradisional, lengkap dengan analisis beban kerja melampau (*extreme concurrency*), keselamatan siber (*zero-trust security*), dan contoh kod integrasi.

---

## 📑 Isi Kandungan

1. [Senario 1: Permainan Berbilang Pemain Masa-Nyata (Real-Time Multiplayer Gaming)](#1-permainan-berbilang-pemain-masa-nyata-real-time-multiplayer-gaming)
2. [Senario 2: Pertahanan Siber & Anti-Bruteforce (Zero-Trust Security Engine)](#2-pertahanan-siber--anti-bruteforce-zero-trust-security-engine)
3. [Senario 3: E-Dagang Trafik Tinggi & Jualan Kilat (High-Concurrency Flash Sales)](#3-e-dagang-trafik-tinggi--jualan-kilat-high-concurrency-flash-sales)
4. [Senario 4: Ejen AI & Multi-Modal GraphRAG (AI-Native RAG Pipelines)](#4-ejen-ai--multi-modal-graphrag-ai-native-rag-pipelines)
5. [Senario 5: Mikroservis Teragih Merentas Benua (Active-Active Geo-Replication)](#5-mikroservis-teragih-merentas-benua-active-active-geo-replication)

---

## 1. Permainan Berbilang Pemain Masa-Nyata (Real-Time Multiplayer Gaming)

Dalam sistem permainan moden (Unity, Unreal Engine, Roblox, Godot, Discord Bot, WebGL), ribuan pemain menghantar status kedudukan, markah, dan aksi secara serentak sesaat.

### 🎮 Bagaimana Ciri Senibina FaizDB Menyelamatkan Beban Game:

| Ciri Senibina FaizDB | Cabaran Permainan Tradisional | Bagaimana FaizDB Mengatasinya |
| :--- | :--- | :--- |
| **Lock-Free MemTable (`crossbeam-skiplist`)** | Pelayan game *freeze* apabila beribu pemain menulis markah serentak (*mutex lock contention*). | Membolehkan beribu-ribu *thread* pelayan game menulis data serentak **tanpa sebarang kunci (Lock-Free)**, mencapai **320,000+ ops/sec**. |
| **gRPC Binary Protocol (Port 50051)** | Format JSON biasa terlalu berat untuk penghantaran telemetri pantas 60 FPS. | Menyokong paket **Protocol Buffers binari HTTP/2** yang sangat ringan dengan latensi **sub-milisaat (< 1ms)**. |
| **WebSocket Change Streams (Port 27018)** | Pemain terpaksa melakukan *polling* berulang kali untuk melihat markah terkini (*high server load*). | Sebarang perubahan markah atau status pemain ditolak (**Push Event**) ke semua pemain dalam bilik perlawanan dalam sekelip mata. |
| **High-Speed TTL In-Memory Engine** | Memori pelayan membengkak dengan data lobi bilik perlawanan yang sudah tamat. | Lobi perlawanan (*Matchmaking Rooms*) dan sesi OTP dipadam secara automatik menggunakan min-heap $O(\log N)$ selepas masa tamat. |
| **Safe Rust Zero-GC (No Garbage Collection)** | Database berasaskan Java / Go mengalami *GC pause* (game tiba-tiba lag 300ms–1s). | Rust menguruskan memori tanpa GC. **Sifar lag spike**, memastikan permainan berjalan lancar dan konsisten. |

#### 💻 Contoh Kod: Kemas Kini Markah & Siaran Langsung (WebSocket Stream):
```python
# Pelayan Game menghantar kemas kini markah pemain (gRPC / Port 50051):
from faizdb import FaizDbGrpcClient

client = FaizDbGrpcClient(target="localhost:50051")

# Update leaderboard pemain dalam masa 0.4 milisaat:
client.execute_query("""
    UPDATE leaderboards 
    SET score = score + 250, kills = kills + 1 
    WHERE player_id = 'player_cyber_99'
""")
```

---

## 2. Pertahanan Siber & Anti-Bruteforce (Zero-Trust Security Engine)

Apabila pangkalan data berdepan cubaan serangan penembusan kata laluan (*Brute-Force*), serangan penafian perkhidmatan (*DDoS/Slowloris*), atau cubaan manipulasi data.

### 🛡️ Matriks Pertahanan Keselamatan:

| Jenis Serangan | Mekanisme Pertahanan FaizDB | Hasil Keselamatan |
| :--- | :--- | :--- |
| **Serangan Brute-Force Kamus / GPU Cluster** | **Argon2id Memory-Hard Hashing** ($m=65536, t=3, p=4$). | Penyerang memerlukan memori RAM fizikal yang besar bagi setiap tekaan; GPU/ASIC brute-forcer lumpuh dan menjadi terlalu perlahan. |
| **Cubaan Log Masuk Berulang Kali** | **Rate Limiter & IP Auto-Blocklist**. | Selepas had kegagalan dicapai, alamat IP penyerang disekat serta-merta pada lapisan TCP gateway. |
| **Serangan Sambungan Perlahan (Slowloris)** | **`TimeoutLayer` 30 Saat Terbina**. | Menutup sambungan tergantung secara automatik untuk melindungi *connection pool* pelayan. |
| **Manipulasi Fail Storan Fizikal** | **Penyulitan AES-256-GCM AEAD & CRC32 WAL Checksums**. | Jika fail diubah suai pada cakera, integriti disahkan gagal dan amaran keselamatan dicetuskan serta-merta. |

---

## 3. E-Dagang Trafik Tinggi & Jualan Kilat (High-Concurrency Flash Sales)

Semasa kempen promosi jualan besar-besaran (seperti 11.11 atau pelancaran tiket konsert), ribuan pengguna membuat tempahan bagi stok barang yang terhad.

### 🛍️ Kelebihan FaizDB:
1. **Multi-Document Snapshot ACID:** Memastikan stok tidak terlebih jual (*no overselling*) melalui transaksi atomik `BEGIN ... COMMIT`.
2. **Secondary B-Tree Unique Constraints:** Menghalang pengeluaran nombor invois atau baucar pendua dengan carian pantas $O(\log N)$.
3. **Point-In-Time Backup (PITR):** Sandaran data kewangan secara *non-blocking* tanpa mengganggu transaksi jualan langsung.

---

## 4. Ejen AI & Multi-Modal GraphRAG (AI-Native RAG Pipelines)

Kebanyakan sistem AI Generatif terpaksa menyambung ke 3 pangkalan data berbeza: satu untuk teks (MongoDB), satu untuk vektor (Pinecone/Qdrant), dan satu untuk graf pengetahuan (Neo4j).

### 🤖 Penyelesaian Bersepadu FaizDB:
* **Satu Pertanyaan Tunggal:** Mengambil profil pelanggan (Dokumen), mencari artikel dokumen yang paling relevan (HNSW Vector Search 4096-dimensi), dan menelusuri hubungan entiti berkaitan (GraphRAG Multi-Hop Traversal) dalam satu pusingan rangkaian (*1 network roundtrip*).

---

## 5. Mikroservis Teragih Merentas Benua (Active-Active Geo-Replication)

Bagi syarikat global dengan pengguna di Asia, Eropah, dan Amerika Syarikat yang memerlukan akses pantas di pusat data tempatan.

### 🌍 Seni Bina CRDTs Multi-Region:
* Pengguna di Singapura (`ap-southeast-1`) dan Frankfurt (`eu-central-1`) boleh menulis data serentak ke nod tempatan masing-masing dalam masa **< 1ms**.
* Enjin CRDTs (*Version Vectors, Last-Write-Wins, OR-Set, PN-Counter*) menyelaraskan data di latar belakang secara automatik tanpa memerlukan kunci teragih (*Zero Distributed Locks*).
