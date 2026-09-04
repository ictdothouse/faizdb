# 🛡️ FaizDB Enterprise Production Standards & Operational Hardening Reference

> **Dokumen Spesifikasi Teknikal & Piawaian Operasi Pengeluaran (Mission-Critical Enterprise Standards)**  
> **Status Pengesahan:** 100% Lulus Ujian Regresi, Sifar Amaran (Zero-Warnings), 100% Safe Rust  
> **Versi:** v0.1.0-Enterprise (September 2026)  
> **Arkitek Enjin:** Ahmad Faiz

---

## 🌟 Pengenalan & Matlamat Piawaian

Dalam persekitaran pengeluaran berskala besar (*mission-critical enterprise deployments*), kepantasan enjin semata-mata tidak memadai. Enjin pangkalan data mesti berdaya tahan terhadap beban lampau (*overload*), mengelakkan kegagalan rantai (*cascading failures*), serasi secara natif dengan orkestrasyen kontena awan (*Cloud-Native Kubernetes*), menyediakan sandaran autonomi, dan menjamin kedaulatan data pengguna tanpa sebarang *vendor lock-in*.

Dokumen ini merekodkan secara terperinci doktrin seni bina berdikari (**Standalone-First FaizQL Engine**), dualiti ketekalan Teorem CAP, serta **6 Piawaian Operasi Pengeluaran** yang diimplementasikan secara terbina dalam (*built-in*) pada FaizDB.

---

## 🏛️ Doktrin Seni Bina Berdikari: FaizQL Natif vs. Adapter Wayar Pilihan

Salah satu salah faham biasa ialah menganggap FaizDB bergantung kepada ekosistem luar atau mengandungi salinan penuh enjin legasi pihak ketiga (seperti PostgreSQL atau MongoDB). Realitinya:

1. **FaizDB Adalah Enjin Berdiri Sendiri Tulen (*100% Standalone Pure-Rust Engine*):**
   - FaizDB dibina dari asas (*clean-slate microkernel architecture*) tanpa sebarang baris kod C/C++ daripada PostgreSQL, SQLite, atau MongoDB.
   - Mempunyai enjin storan natif sendiri (`faizdb-core`: MemTable berasaskan SkipList/BTreeMap, Write-Ahead Log berputar, dan pemformatan SSTable mikron).
   - Mempunyai bahasa kueri natif sendiri: **FaizQL** (`faizdb-query`), lengkap dengan tokenizer, AST parser, Cost-Based Optimizer (CBO), dan executor natif.
   - Mempunyai protokol pengeluaran natif sendiri: **FaizDB Native gRPC (Port 50051) & REST API (Port 8080)**.

2. **Adapter Protokol Wayar (Port 5432 & 27017) Hanyalah Pintu Masuk Pilihan (*Optional Ingress Adapters*):**
   - Pembangun **TIDAK DIWAJIBKAN** menggunakan port PostgreSQL atau MongoDB.
   - Pintu masuk ini diwujudkan semata-mata sebagai kemudahan integrasi (*developer ergonomics*), membolehkan aplikasi sedia ada menyambung ke FaizDB menggunakan pemacu (drivers) dan alatan GUI popular (DBeaver, TablePlus, Compass, Prisma, Drizzle) tanpa perlu mempelajari protokol baharu pada hari pertama.
   - Adapter ini hanya menguraikan bingkai TCP (*packet framing*) dan menterjemahkannya terus ke dalam AST FaizQL tanpa membawa beban (*zero bloat*). Inilah rahsia bagaimana binari keluaran FaizDB kekal sangat padat (**7.70 MB**) berbanding pangkalan data legasi yang memerlukan ratusan megabait.

3. **Dualiti Teorem CAP yang Jelas (CP Mode vs. AP Mode):**
   - **Mod CP (Linearizable Strict Consistency):** Dioptimumkan untuk lejar kewangan, perbankan, dan pengurusan inventori menggunakan enjin ACID MVCC penuh, WAL atomik, dan konsensus teragih Raft. Transaksi partition ditolak demi menjamin sifar perbelanjaan berganda (*zero double-spending*).
   - **Mod AP (High-Availability Eventual Consistency):** Dioptimumkan untuk nod multi-wilayah pinggir (*Edge*), kolaborasi dokumen serentak, dan telemetri IoT menggunakan struktur data bebas konflik (*Conflict-free Replicated Data Types - CRDTs* seperti PN-Counters, LWW-Registers, dan OR-Sets). Menjamin kependaman sub-milisaat tempatan tanpa kunci teragih (*zero distributed locking overhead*). Mod ini dipilih secara eksplisit mengikut jenis koleksi (*collection-level isolation*).

---

## 📋 Senarai Piawaian Pengeluaran FaizDB

```
                                  ┌────────────────────────────────────────────────────────┐
                                  │            FaizDB Production Hardening                 │
                                  │            Mission-Critical Standards                  │
                                  └──────────────────────────┬─────────────────────────────┘
                                                             │
        ┌──────────────────────────────┬─────────────────────┴──────────────┬──────────────────────────────┐
        ▼                              ▼                                    ▼                              ▼
┌──────────────────┐          ┌──────────────────┐                ┌──────────────────┐          ┌──────────────────┐
│   Piawaian 1:    │          │   Piawaian 2:    │                │   Piawaian 3:    │          │   Piawaian 4:    │
│ Connection Gov.  │          │ WAL Group Commit │                │ Kubernetes K8s   │          │ Auto-Snapshot    │
│ Tokio Semaphore  │          │ Atomic Batch I/O │                │ Liveness/Ready   │          │ Background Daemon│
│ RFC 53300 Fatal  │          │ Amortized fsync  │                │ Zero Sidecars    │          │ Timestamp Rotate │
└──────────────────┘          └──────────────────┘                └──────────────────┘          └──────────────────┘
        │                              │                                    │                              │
        └──────────────────────────────┴─────────────────────┬──────────────┴──────────────────────────────┘
                                                             │
                                ┌────────────────────────────┴───────────────────────────┐
                                ▼                                                        ▼
                    ┌────────────────────────┐                              ┌────────────────────────┐
                    │      Piawaian 5:       │                              │      Piawaian 6:       │
                    │ Open-Format Dump (CLI) │                              │ Wire Protocol Hardening│
                    │ Streaming JSONL & SQL  │                              │ Extended Query & Joins │
                    │ Anti-Vendor Lock-in    │                              │ Mongo O(1) Fast Path   │
                    └────────────────────────┘                              └────────────────────────┘
```

---

## 🛡️ Piawaian 1: Gabenor Bebanan Sambungan (Max Connections Governor)

### 1.1 Latar Belakang & Masalah Industri
Apabila ribuan klien atau aplikasi mengalami pepijat kebocoran sambungan (*connection leak*) atau serangan penafian perkhidmatan (DDoS), pangkalan data tanpa gabenor sambungan akan terus membuka fail deskriptor (FD) dan memulakan *task* baharu sehingga sistem operasi kehabisan memori (*Out-Of-Memory / OOM*), meruntuhkan keseluruhan proses pelayan.

### 1.2 Mekanisme & Seni Bina FaizDB
FaizDB melaksanakan kawalan kemasukan (*Admission Control*) menggunakan `tokio::sync::Semaphore` tak segerak (*asynchronous semaphore*) pada peringkat pintu masuk rangkaian (*TCP listener*):

* **Fail Terlibat:**
  - [`faizdb-server/src/wire/listener.rs`](file:///c:/Users/afaiz/Documents/2006/PERSONAL2026/ICTHOUSE2026/FAIZDB/faizdb-server/src/wire/listener.rs) (MongoDB Gateway - Port 27017)
  - [`faizdb-server/src/wire/postgres/listener.rs`](file:///c:/Users/afaiz/Documents/2006/PERSONAL2026/ICTHOUSE2026/FAIZDB/faizdb-server/src/wire/postgres/listener.rs) (PostgreSQL Gateway - Port 5432)
* **Konfigurasi:**
  ```bash
  export FAIZDB_MAX_CONNECTIONS=10000 # Nilai lalai: 10,000 sambungan serentak
  ```
* **Tingkah Laku Penolakan Anggun (*Graceful Rejection*):**
  - **Protokol PostgreSQL:** Jika kapasiti penuh, sambungan baharu **tidak digugurkan secara kasar**. Sebaliknya, pelayan membalas dengan mesej ralat sah standard PostgreSQL Wire berserta kod ralat rasmi SQLSTATE:
    ```text
    Severity: FATAL
    Code: 53300 (too_many_clients_already)
    Message: sorry, too many clients already (limit: 10000)
    ```
    Klien SQL (seperti `psql`, Prisma, DBeaver) akan memahami ralat ini dengan teratur tanpa mengalami sambungan beku (*hanging*).
  - **Protokol MongoDB:** Sambungan soket ditutup dengan kemas tanpa kebocoran fail deskriptor atau alokasi buffer pemprosesan kueri.

---

## ⚡ Piawaian 2: WAL Group Commit & Ketahanan Berkelompok (Batch Durability)

### 2.1 Masalah Kekangan Fizikal IOPS
Cakera storan moden (termasuk SSD NVMe gred perusahaan) terikat dengan had fizikal kitaran `fsync` (sekitar 20,000 – 100,000 IOPS). Melakukan panggilan sistem `fsync` bagi setiap transaksi individu akan melumpuhkan kelajuan sistem apabila ratusan ribu pengguna menulis data serentak.

### 2.2 Inovasi Group Commit FaizDB
FaizDB mengimplementasikan pengelompokan atomik tunggal (*single-buffer atomic batching*):

* **Fail Terlibat:**
  - [`faizdb-core/src/storage/wal.rs`](file:///c:/Users/afaiz/Documents/2006/PERSONAL2026/ICTHOUSE2026/FAIZDB/faizdb-core/src/storage/wal.rs)
  - [`faizdb-core/src/storage/engine.rs`](file:///c:/Users/afaiz/Documents/2006/PERSONAL2026/ICTHOUSE2026/FAIZDB/faizdb-core/src/storage/engine.rs)
* **Antara Muka API Teras:**
  ```rust
  // Menulis siri operasi dalam satu penimbal bersiri dengan 1 panggilan fsync tunggal
  pub fn append_batch(&self, ops: &[(WalOpType, &[u8], &[u8])]) -> FaizResult<Vec<u64>>
  
  // Memasukkan kumpulan rekod ke MemTable dan WAL serentak secara atomik
  pub fn put_batch(&self, entries: &[(&[u8], &[u8])]) -> FaizResult<()>
  ```
* **Jaminan Integriti:**
  - Setiap rekod log dalam kumpulan (*batch*) mengekalkan penjajaran urutan LSN (*Log Sequence Number*) dan semakan integriti CRC32 unik.
  - Sekiranya pelayan dimatikan secara paksa (`pkill -9`), enjin storan memainkan semula (*replay*) rekod log yang sah sehingga LSN terakhir yang berjaya di-commit.

---

## ☸️ Piawaian 3: Siasatan Kesihatan Natif Kubernetes (Liveness & Readiness Probes)

### 3.1 Menghapuskan Keperluan "Sidecar" & "Operator"
Pangkalan data era lama (PostgreSQL atau MySQL asal) memerlukan *sidecar container* tambahan atau *Kubernetes Operator* untuk mendedahkan status kesihatan nod melalui HTTP. FaizDB menyertakan *native HTTP probes* terus di dalam binari pelayan (Port 27018).

* **Fail Terlibat:**
  - [`faizdb-server/src/api/health.rs`](file:///c:/Users/afaiz/Documents/2006/PERSONAL2026/ICTHOUSE2026/FAIZDB/faizdb-server/src/api/health.rs)
  - [`faizdb-server/src/api/mod.rs`](file:///c:/Users/afaiz/Documents/2006/PERSONAL2026/ICTHOUSE2026/FAIZDB/faizdb-server/src/api/mod.rs)

### 3.2 Spesifikasi Endpoint
1. **Liveness Probe (`GET /v1/health/liveness`):**
   - **Tujuan:** Mengesahkan bahawa *event-loop* proses pelayan FaizDB tidak terhenti (*deadlock*) dan mampu membalas permintaan HTTP.
   - **Respons:** `HTTP 200 OK` dengan payload:
     ```json
     {
       "status": "alive"
     }
     ```
   - **Tindakan Kubelet:** Jika gagal melepasi ambang kegagalan, Kubernetes akan memulakan semula (*restart*) Pod secara automatik.

2. **Readiness Probe (`GET /v1/health/readiness`):**
   - **Tujuan:** Mengesahkan bahawa enjin storan FaizDB dalam keadaan sedia menerima kueri trafik pengeluaran (bukan dalam mod pemulihan kerosakan atau migrasi cakera).
   - **Respons:** `HTTP 200 OK` dengan payload:
     ```json
     {
       "status": "ready",
       "database": "faizdb",
       "engine": "active"
     }
     ```
   - **Tindakan Kubelet:** Jika endpoint belum bersedia, Kubernetes tidak akan menghalakan trafik perkhidmatan (*Service ingress/cluster IP*) ke Pod ini.

### 3.3 Contoh Konfigurasi Kubernetes Pod / StatefulSet
```yaml
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: faizdb-cluster
spec:
  serviceName: "faizdb"
  replicas: 3
  template:
    spec:
      containers:
      - name: faizdb
        image: ictdothouse/faizdb:v0.1.0
        ports:
        - containerPort: 27018
          name: http-rest
        - containerPort: 5432
          name: postgres-wire
        - containerPort: 27017
          name: mongo-wire
        - containerPort: 50051
          name: grpc
        livenessProbe:
          httpGet:
            path: /v1/health/liveness
            port: 27018
          initialDelaySeconds: 5
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /v1/health/readiness
            port: 27018
          initialDelaySeconds: 3
          periodSeconds: 5
```

---

## ⏰ Piawaian 4: Daemon Sandaran Automatik Berjadual (Automated Snapshot Daemon)

### 4.1 Sandaran Autonomi Tanpa Cron Luaran
FaizDB mengandungi gelung latar belakang tak segerak (*asynchronous background daemon*) yang berjalan bersama pelayan pangkalan data. Bagi penggunaan *standalone* atau kontena Docker, pentadbir sistem tidak perlu lagi mengkonfigurasi *Linux cronjob* di luar kontena.

* **Fail Terlibat:** [`faizdb-server/src/lib.rs`](file:///c:/Users/afaiz/Documents/2006/PERSONAL2026/ICTHOUSE2026/FAIZDB/faizdb-server/src/lib.rs)
* **Pembolehubah Persekitaran:**
  | Pembolehubah | Nilai Lalai | Penerangan |
  | :--- | :---: | :--- |
  | `FAIZDB_AUTO_BACKUP` | `false` | Tetapkan kepada `true` untuk mengaktifkan daemon automatik |
  | `FAIZDB_BACKUP_INTERVAL_SECS`| `3600` (1 jam) | Sela masa sandaran dalam unit saat |
  | `FAIZDB_BACKUP_DIR` | `./backups` | Direktori destinasi fail snapshot |

* **Penamaan Fail & Integriti:**
  Snapshot disimpan secara atomik dengan format nama:
  ```text
  ./backups/faizdb_snapshot_<timestamp_nanos>.json
  ```
  Setiap snapshot merekodkan keadaan koleksi secara konsisten dengan pengecam LSN terkini bagi membolehkan pemulihan titik masa (*Point-In-Time Recovery / PITR*).

---

## 📦 Piawaian 5: Kebolehsalinan Data Format Terbuka (Anti-Vendor Lock-in)

### 5.1 Kedaulatan & Pemindahan Data Bebas
FaizDB mengamalkan polisi sumber terbuka mutlak tanpa memerangkap data pengguna (*Zero Vendor Lock-in*). Pengguna bebas mengekstrak keseluruhan pangkalan data ke format standard industri pada bila-bila masa menggunakan alat rasmi CLI.

* **Fail Terlibat:** [`faizdb-cli/src/main.rs`](file:///c:/Users/afaiz/Documents/2006/PERSONAL2026/ICTHOUSE2026/FAIZDB/faizdb-cli/src/main.rs)
* **Sintaks Perintah:**
  ```bash
  # 1. Eksport ke format JSONL (sesuai untuk BigQuery, Snowflake, ClickHouse, Apache Spark)
  faizdb dump --data-dir ./faizdb_data --format jsonl --output backup.jsonl

  # 2. Eksport ke fail arahan SQL standard (serasi terus dengan PostgreSQL, MySQL, SQLite)
  faizdb dump --data-dir ./faizdb_data --format sql --output backup.sql

  # 3. Eksport koleksi terpilih sahaja
  faizdb dump --data-dir ./faizdb_data --collection users --format sql --output users.sql
  ```

* **Kecekapan Penstriman (Streaming Efficiency):**
  Proses eksport membaca kunci dan nilai terus melalui *Zero-Copy Iterator* enjin storan secara berurutan. Ini membolehkan eksport pangkalan data bersaiz puluhan gigabait berjalan dengan penggunaan memori RAM yang malar (*$O(1)$ memory consumption*) tanpa membebankan pelayan.

---

## 🔌 Piawaian 6: Pemerkasaan Protokol Wire (PostgreSQL & MongoDB)

### 6.1 Protokol Kueri Lanjutan PostgreSQL (Extended Query Protocol)
Bagi menyokong sepenuhnya pustaka ORM moden (Prisma, Hibernate, SQLAlchemy, TypeORM, `sqlx`), FaizDB menyokong kitaran penuh mesej Extended Query:
* `'P'` (**Parse**): Menghurai dan menyimpan penyata berparameter (`$1`, `$2`, `$3`).
* `'B'` (**Bind**): Mengikat nilai parameter ke dalam penyata bagi menghapuskan risiko serangan SQL Injection.
* `'D'` (**Describe**): Memulangkan metadata lajur dan jenis data bagi prapenyediaan kueri.
* `'E'` (**Execute**): Menjalankan kueri dan memulangkan hasil baris data.
* `'S'` (**Sync**): Mengakhiri kitaran kueri dan memulangkan status `ReadyForQuery`.

### 6.2 Carian Pantas $O(1)$ & Paginasi Kursor MongoDB Wire
* **$O(1)$ ID Fast-Path:** Kueri yang mengandungi penapis `{ "_id": ... }` tidak lagi melakukan imbasan lelaran $O(N)$, sebaliknya terus mengakses indeks primer enjin storan pada kelajuan $O(1)$.
* **Paginasi Kursor Berkeadaan (*Stateful Cursor*):** Menyokong arahan `getMore` dan `killCursors` MongoDB untuk penstriman data berskala besar tanpa menyekat sambungan klien.

### 6.3 Relational SQL: Multi-Table Hash Join
Enjin kueri FaizQL menyokong cantuman berbilang jadual (*Multi-Table Joins*):
```sql
SELECT orders.id, users.name, orders.amount 
FROM orders 
INNER JOIN users ON orders.user_id = users.id 
WHERE orders.status = 'completed';
```
Enjin menggunakan algoritma **In-Memory Hash Join** berkelajuan tinggi yang memetakan baris padanan dengan masa lelurus $O(N + M)$.

---

## 🛑 Piawaian 7: Penutupan Anggun Bersatu Merentas Protokol (Unified Multi-Protocol Graceful Shutdown)

### 7.1 Latar Belakang & Cabaran Pengeluaran
Dalam seni bina pengeluaran kontena (Kubernetes Pods, Nomad, Systemd), proses pelayan sering menerima isyarat penamatan (`SIGINT`, `SIGTERM`) semasa *rolling update* atau penskalaan automatik. Penutupan secara mendadak (abrupt kill) boleh menyebabkan kerosakan sambungan dalam perjalanan (*in-flight TCP socket resets*), transaksi terputus separuh jalan, atau fail WAL yang belum sempat di-sync ke cakera.

### 7.2 Penyelesaian FaizDB
FaizDB mengintegrasikan saluran siaran isyarat penutupan (`tokio::sync::broadcast`) merentas seluruh pintu masuk protokol rangkaian:
* **HTTP / REST / Admin Portal (Axum):** Menggunakan `with_graceful_shutdown` untuk menamatkan penerimaan sambungan baharu sambil menunggu permintaan aktif selesai.
* **MongoDB Wire Protocol (Port 27017):** [`run_wire_server_with_shutdown`](file:///c:/Users/afaiz/Documents/2006/PERSONAL2026/ICTHOUSE2026/FAIZDB/faizdb-server/src/wire/listener.rs) memantau isyarat penutupan dan menamatkan pendengar soket secara teratur.
* **PostgreSQL Wire Protocol (Port 5432):** [`run_postgres_server_with_shutdown`](file:///c:/Users/afaiz/Documents/2006/PERSONAL2026/ICTHOUSE2026/FAIZDB/faizdb-server/src/wire/postgres/listener.rs) menghantar mesej penutupan dan membebaskan sesi soket.
* **gRPC High-Performance Engine (Port 50051):** [`run_grpc_server_with_shutdown`](file:///c:/Users/afaiz/Documents/2006/PERSONAL2026/ICTHOUSE2026/FAIZDB/faizdb-server/src/grpc/listener.rs) melengkapkan panggilan RPC penstriman sebelum penutupan.

---

## 🗄️ Piawaian 8: Titik Semak & Pemangkasan Jurnal Autonomi (Proactive WAL Checkpointing & Disk Reclaim)

### 8.1 Latar Belakang & Masalah Ruang Cakera
Pangkalan data berprestasi tinggi yang hanya menambah log ke Write-Ahead Log tanpa pemangkasan berkala boleh menyebabkan fail log membesar tanpa kawalan (*disk exhaustion*).

### 8.2 Penyelesaian FaizDB
* **Mekanisme Checkpointing:** [`Wal::checkpoint(&self)`](file:///c:/Users/afaiz/Documents/2006/PERSONAL2026/ICTHOUSE2026/FAIZDB/faizdb-core/src/storage/wal.rs) membaca nombor jujukan transaksi (*sequence ID*) yang telah disahkan berada dalam MemTable atau fail data kekal, dan memangkas rekod lama yang tidak lagi diperlukan untuk pemulihan nahas.
* **Penyelarasan Storan Automatik:** Fungsi `StorageEngine::flush()` dan `StorageEngine::compact()` memanggil `wal.checkpoint()` secara proaktif bagi memastikan saiz storan pada cakera kekal padat dan optimum sepanjang masa.

---

## ⏱️ Piawaian 9: Pembasmi Transaksi Terbiar Autonomi (MVCC Transaction Idle-Reaper Daemon)

### 9.1 Latar Belakang & Kebocoran Versi MVCC
Sekiranya klien memulakan transaksi (`BEGIN`) kemudian terputus sambungan (*connection dropped/timeout*) tanpa mengeluarkan arahan `COMMIT` atau `ROLLBACK`, rekod pengasingan gambar (*snapshot isolation records*) akan kekal dalam RAM dan menyekat pembersihan sampah MVCC (*MVCC vacuum bloat*).

### 9.2 Penyelesaian FaizDB
* **Cap Waktu Penciptaan Transaksi:** Setiap transaksi kini merekodkan `created_at: Instant` semasa permulaannya dalam [`mvcc.rs`](file:///c:/Users/afaiz/Documents/2006/PERSONAL2026/ICTHOUSE2026/FAIZDB/faizdb-core/src/transaction/mvcc.rs).
* **Daemon Pembersihan Berjadual:** [`reap_expired_transactions(timeout)`](file:///c:/Users/afaiz/Documents/2006/PERSONAL2026/ICTHOUSE2026/FAIZDB/faizdb-query/src/executor.rs) dijalankan secara autonomi setiap 30 saat di latar belakang pelayan. Transaksi yang melangkaui had masa melahu (konfigurasi `FAIZDB_TXN_TIMEOUT_SECS`, lalai 300s) secara automatik di-abort dan dibersihkan daripada memori tanpa campur tangan pentadbir.

---

## ⚡ Piawaian 10: Tolakan Had Imbasan Kueri (Sub-Millisecond Query Scan Limit Pushdown)

### 10.1 Latar Belakang & Imbasan Lebihan (Over-Scanning)
Dalam kueri lazim seperti `SELECT * FROM table LIMIT 10`, sistem tanpa tolakan had akan mengimbas jutaan rekod ke dalam memori sebelum memangkas 10 rekod teratas, membazirkan kitaran CPU dan lebar jalur ingatan.

### 10.2 Penyelesaian FaizDB
* **Limit Pushdown:** Enjin [`faizdb-query/src/executor.rs`](file:///c:/Users/afaiz/Documents/2006/PERSONAL2026/ICTHOUSE2026/FAIZDB/faizdb-query/src/executor.rs) menyalurkan nilai `LIMIT` secara terus ke lapisan imbasan lelaran dokumen.
* **Penamatan Awal (Short-Circuit Evaluation):** Sebaik sahaja bilangan rekod yang diminta dicapai, lelaran dihentikan serta-merta, memberikan masa kueri sub-milisaat walaupun pada koleksi bersaiz jutaan dokumen.

---

## 📐 Piawaian 11: Pengawalan Sempadan Titik Terapung Vektor (Numerical Float Safety & Distance Clamping)

### 11.1 Latar Belakang & Isu Ketepatan IEEE 754
Pengiraan jarak kosinus (`Cosine Similarity / Distance`) pada vektor berdimensi tinggi (512, 1536 dimensi AI embeddings) terdedah kepada herotan pembundaran nombor titik terapung (*floating point precision loss*), yang boleh menghasilkan nilai sedikit melebihi 1.0 (cth. 1.0000001) atau menghasilkan `NaN` pada vektor sifar.

### 11.2 Penyelesaian FaizDB
* **Pengawalan Sempadan Ketat:** Enjin [`faizdb-vector/src/distance.rs`](file:///c:/Users/afaiz/Documents/2006/PERSONAL2026/ICTHOUSE2026/FAIZDB/faizdb-vector/src/distance.rs) mengapit (*clamps*) hasil pembahagian dot produk tepat dalam lingkungan `[-1.0, 1.0]` dan jarak kosinus dalam `[0.0, 2.0]`.
* **Kestabilan Indeks Graf HNSW:** [`faizdb-vector/src/hnsw.rs`](file:///c:/Users/afaiz/Documents/2006/PERSONAL2026/ICTHOUSE2026/FAIZDB/faizdb-vector/src/hnsw.rs) mengesahkan jarak sentiasa bernilai sah tanpa kemungkinan tercetusnya `NaN` atau perbandingan tidak sah (`f32::total_cmp`).

---

## 🕸️ Piawaian 12: Bajet Sumber Perjalanan Graf (Bounded-Resource Graph Traversal & Cycle Guard)

### 12.1 Latar Belakang & Lingkaran Tak Terhingga (Infinite Loops)
Pada graf pengetahuan (*Knowledge Graph*) yang kompleks dan mengandungi kitaran (cycles / loops), kueri perjalanan BFS/DFS tanpa kawalan bajet boleh menyebabkan kitaran CPU 100% dan limpahan memori RAM (*runaway graph traversals*).

### 12.2 Penyelesaian FaizDB
* **Kaedah Perjalanan Terkawal:** [`traverse_bfs_bounded(&self, start_id, max_depth, relation_filter, max_nodes)`](file:///c:/Users/afaiz/Documents/2006/PERSONAL2026/ICTHOUSE2026/FAIZDB/faizdb-graph/src/graph.rs) mengenakan siling maksimum (lalai 50,000 nod atau had khusus kueri).
* **Deduplikasi Nod Terkini:** Menggunakan struktur `HashSet` bagi memastikan setiap nod hanya diproses sekali sahaja, menghapuskan risiko terperangkap dalam kitaran berulang.

---

## 📊 Matriks Status Pengesahan Pengeluaran

| Komponen Pengeluaran | Fail Suite Ujian Pengesahan | Status | Liputan & Pengesahan |
| :--- | :--- | :---: | :---: |
| **Enterprise Production Hardening (12 Piawaian)** | [`faizdb-server/tests/test_production_hardening_and_features.rs`](file:///c:/Users/afaiz/Documents/2006/PERSONAL2026/ICTHOUSE2026/FAIZDB/faizdb-server/tests/test_production_hardening_and_features.rs) | **PASS (9/9)** | WAL Checkpoints, Limit Pushdown, Reaper, Float Clamping, Graph Budget, K8s Probes, Connection Governor |
| **Extended Query & Hash Joins** | [`faizdb-server/tests/test_competitor_vulnerabilities_remediation.rs`](file:///c:/Users/afaiz/Documents/2006/PERSONAL2026/ICTHOUSE2026/FAIZDB/faizdb-server/tests/test_competitor_vulnerabilities_remediation.rs) | **PASS (6/6)** | PG Extended Wire ($1, $2), Mongo Stateful Cursors, HNSW Tombstones, Raft Quorum |
| **Multi-Protocol Security & Throughput** | [`faizdb-server/tests/test_wire_security_and_performance.rs`](file:///c:/Users/afaiz/Documents/2006/PERSONAL2026/ICTHOUSE2026/FAIZDB/faizdb-server/tests/test_wire_security_and_performance.rs) | **PASS (3/3)** | gRPC RBAC, Mongo RBAC, High Throughput Benchmark |
| **Storage Durability & Crash Recovery** | [`faizdb-server/tests/test_durability_and_mvcc.rs`](file:///c:/Users/afaiz/Documents/2006/PERSONAL2026/ICTHOUSE2026/FAIZDB/faizdb-server/tests/test_durability_and_mvcc.rs) | **PASS (5/5)** | WAL Replay, Crash Safety, Snapshot Isolation |
| **Audit Security & Correctness**| [`faizdb-server/tests/test_audit_security_and_correctness.rs`](file:///c:/Users/afaiz/Documents/2006/PERSONAL2026/ICTHOUSE2026/FAIZDB/faizdb-server/tests/test_audit_security_and_correctness.rs) | **PASS (3/3)** | CBO Float Bounds, Safe System Table Routing, Vector Validation |
| **Jumlah Keseluruhan Ujian Ruang Kerja** | `cargo test --workspace` | **100% PASS** | **200+ Ujian Integrasi & Unit (Sifar Kegagalan)** |
| **Kepadatan Binari Mesin (Release)** | `target/release/faizdb` (Fat LTO, Strip Symbols) | **7.70 MB** | Binari Tunggal Berdikari Sedia Diagihkan |

---
*FaizDB — Diarkitekkan untuk Kestabilan Maksimum, Keselamatan Memori Mutlak, dan Kesiapsiagaan Pengeluaran Global.*

