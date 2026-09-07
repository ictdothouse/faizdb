//! Comprehensive integration tests for Automated Tiered Storage in FaizDB StorageEngine.

use faizdb_core::storage::engine::{StorageConfig, StorageEngine};
use faizdb_core::storage::tiered::TieredStorageConfig;
use tempfile::tempdir;

#[test]
fn test_tiered_storage_initialization_and_telemetry() {
    let dir = tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let hot_dir = data_dir.join("sst");
    let cold_dir = data_dir.join("cold");

    let tiered_cfg = TieredStorageConfig {
        hot_dir: hot_dir.clone(),
        cold_dir: Some(cold_dir.clone()),
        max_hot_bytes: 10 * 1024 * 1024, // 10MB
        cold_migration_age_days: 30,
        enable_auto_tiering: true,
    };

    let storage_cfg = StorageConfig {
        data_dir: data_dir.clone(),
        memtable_size: 1024,
        tiered_storage: Some(tiered_cfg),
        ..Default::default()
    };

    let engine = StorageEngine::open(storage_cfg).expect("Failed to open engine with tiered storage");
    let stats = engine.tiered_stats().expect("Tiered stats should be available");
    assert_eq!(stats.hot_sstable_count, 0);
    assert_eq!(stats.cold_sstable_count, 0);
    assert_eq!(stats.total_hot_bytes, 0);
    assert_eq!(stats.total_cold_bytes, 0);

    let storage_stats = engine.stats();
    assert_eq!(storage_stats.sstable_count, 0);
    assert_eq!(storage_stats.cold_sstable_count, 0);
}

#[test]
fn test_transparent_point_lookup_across_hot_and_cold_tiers() {
    let dir = tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let hot_dir = data_dir.join("sst");
    let cold_dir = data_dir.join("cold");

    let tiered_cfg = TieredStorageConfig {
        hot_dir: hot_dir.clone(),
        cold_dir: Some(cold_dir.clone()),
        max_hot_bytes: 50, // Ultra-low threshold to trigger immediate qualification
        cold_migration_age_days: 0,
        enable_auto_tiering: false, // Manual trigger to verify step-by-step
    };

    let storage_cfg = StorageConfig {
        data_dir: data_dir.clone(),
        memtable_size: 10_000,
        tiered_storage: Some(tiered_cfg),
        ..Default::default()
    };

    let engine = StorageEngine::open(storage_cfg).expect("Open failed");

    // 1. Ingest batch of keys and flush to SSTable
    for i in 1..=5 {
        let k = format!("cold_key_{i}").into_bytes();
        let v = format!("historical_value_{i}").into_bytes();
        engine.put(&k, &v).unwrap();
    }
    engine.flush().unwrap();

    // Verify SSTable exists in hot tier before migration
    assert_eq!(engine.stats().sstable_count, 1);
    assert_eq!(engine.stats().cold_sstable_count, 0);

    // 2. Trigger tier migration to Cold storage
    let migrated = engine.trigger_tier_migration().unwrap();
    assert_eq!(migrated, 1, "SSTable should qualify and migrate to cold");

    let tiered_stats = engine.tiered_stats().unwrap();
    assert_eq!(tiered_stats.hot_sstable_count, 0);
    assert_eq!(tiered_stats.cold_sstable_count, 1);
    assert!(tiered_stats.total_cold_bytes > 0);

    // Verify physical file location
    let cold_files: Vec<_> = std::fs::read_dir(&cold_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(cold_files.len(), 1, "Cold directory should contain migrated SSTable");

    // 3. Ingest new active hot keys
    for i in 6..=10 {
        let k = format!("hot_key_{i}").into_bytes();
        let v = format!("active_value_{i}").into_bytes();
        engine.put(&k, &v).unwrap();
    }

    // 4. Transparent point lookups:
    // Read from Hot tier (active memtable)
    let hot_val = engine.get(b"hot_key_7").unwrap();
    assert_eq!(hot_val, Some(b"active_value_7".to_vec()));

    // Read from Cold tier (migrated SSTable in cold_dir)
    let cold_val = engine.get(b"cold_key_3").unwrap();
    assert_eq!(cold_val, Some(b"historical_value_3".to_vec()), "Transparent cold lookup must succeed");

    // Verify block cache now contains the cold key
    let cached_val = engine.get(b"cold_key_3").unwrap();
    assert_eq!(cached_val, Some(b"historical_value_3".to_vec()));

    let non_existent = engine.get(b"unknown_key_999").unwrap();
    assert_eq!(non_existent, None);
}

#[test]
fn test_transparent_prefix_scan_across_hybrid_tiers() {
    let dir = tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let hot_dir = data_dir.join("sst");
    let cold_dir = data_dir.join("cold");

    let tiered_cfg = TieredStorageConfig {
        hot_dir: hot_dir.clone(),
        cold_dir: Some(cold_dir.clone()),
        max_hot_bytes: 10,
        cold_migration_age_days: 0,
        enable_auto_tiering: true,
    };

    let storage_cfg = StorageConfig {
        data_dir: data_dir.clone(),
        memtable_size: 1000,
        tiered_storage: Some(tiered_cfg),
        ..Default::default()
    };

    let engine = StorageEngine::open(storage_cfg).expect("Open failed");

    // Ingest older sensor readings and migrate to cold
    engine.put(b"sensor:temp:001", b"21.5").unwrap();
    engine.put(b"sensor:temp:002", b"22.0").unwrap();
    engine.flush().unwrap();
    engine.trigger_tier_migration().unwrap();

    // Ingest newer sensor readings into hot tier
    engine.put(b"sensor:temp:003", b"23.1").unwrap();
    engine.put(b"sensor:temp:004", b"24.5").unwrap();

    // Prefix scan across hybrid tiers
    let results = engine.prefix_scan(b"sensor:temp:").unwrap();
    assert_eq!(results.len(), 4, "Prefix scan must combine Cold and Hot records");
    assert_eq!(results[0], (b"sensor:temp:001".to_vec(), b"21.5".to_vec()));
    assert_eq!(results[1], (b"sensor:temp:002".to_vec(), b"22.0".to_vec()));
    assert_eq!(results[2], (b"sensor:temp:003".to_vec(), b"23.1".to_vec()));
    assert_eq!(results[3], (b"sensor:temp:004".to_vec(), b"24.5".to_vec()));
}

#[test]
fn test_cold_sstable_persistence_and_reopen() {
    let dir = tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let hot_dir = data_dir.join("sst");
    let cold_dir = data_dir.join("cold");

    let tiered_cfg = TieredStorageConfig {
        hot_dir: hot_dir.clone(),
        cold_dir: Some(cold_dir.clone()),
        max_hot_bytes: 10,
        cold_migration_age_days: 0,
        enable_auto_tiering: true,
    };

    let storage_cfg = StorageConfig {
        data_dir: data_dir.clone(),
        memtable_size: 1000,
        tiered_storage: Some(tiered_cfg.clone()),
        ..Default::default()
    };

    // 1. Populate and migrate to cold
    {
        let engine = StorageEngine::open(storage_cfg.clone()).unwrap();
        engine.put(b"archival:record:A", b"payload_alpha").unwrap();
        engine.put(b"archival:record:B", b"payload_beta").unwrap();
        engine.flush().unwrap();
        engine.trigger_tier_migration().unwrap();
        engine.close().unwrap();
    }

    // 2. Reopen engine and verify cold SSTables are rediscovered automatically
    {
        let engine = StorageEngine::open(storage_cfg).unwrap();
        let stats = engine.tiered_stats().unwrap();
        assert_eq!(stats.cold_sstable_count, 1, "Cold SSTables must be loaded upon reboot");

        let val_a = engine.get(b"archival:record:A").unwrap();
        assert_eq!(val_a, Some(b"payload_alpha".to_vec()));

        let val_b = engine.get(b"archival:record:B").unwrap();
        assert_eq!(val_b, Some(b"payload_beta".to_vec()));
    }
}

#[test]
fn test_automatic_tier_migration_on_flush() {
    let dir = tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let hot_dir = data_dir.join("sst");
    let cold_dir = data_dir.join("cold");

    let tiered_cfg = TieredStorageConfig {
        hot_dir: hot_dir.clone(),
        cold_dir: Some(cold_dir.clone()),
        max_hot_bytes: 10,
        cold_migration_age_days: 0,
        enable_auto_tiering: true,
    };

    let storage_cfg = StorageConfig {
        data_dir: data_dir.clone(),
        memtable_size: 1000,
        tiered_storage: Some(tiered_cfg),
        ..Default::default()
    };

    let engine = StorageEngine::open(storage_cfg).unwrap();
    engine.put(b"auto:hot_or_cold:1", b"val_1").unwrap();
    engine.put(b"auto:hot_or_cold:2", b"val_2").unwrap();
    engine.flush().unwrap();

    let stats = engine.stats();
    assert_eq!(stats.sstable_count, 0, "Hot SSTable should have auto-migrated");
    assert_eq!(stats.cold_sstable_count, 1, "Cold SSTable should be present");

    let val = engine.get(b"auto:hot_or_cold:1").unwrap();
    assert_eq!(val, Some(b"val_1".to_vec()));
}

#[test]
fn test_hot_compaction_preserves_tombstones_preventing_cold_zombie_resurrection() {
    let dir = tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let hot_dir = data_dir.join("sst");
    let cold_dir = data_dir.join("cold");

    let tiered_cfg = TieredStorageConfig {
        hot_dir: hot_dir.clone(),
        cold_dir: Some(cold_dir.clone()),
        max_hot_bytes: 1_000_000, // Do not auto-migrate so we can control migration manually
        cold_migration_age_days: 0,
        enable_auto_tiering: false,
    };

    let storage_cfg = StorageConfig {
        data_dir: data_dir.clone(),
        memtable_size: 10_000,
        tiered_storage: Some(tiered_cfg),
        ..Default::default()
    };

    let engine = StorageEngine::open(storage_cfg).unwrap();

    // 1. Write user:101 and migrate to Cold
    engine.put(b"user:101", b"alice").unwrap();
    engine.flush().unwrap();
    engine.trigger_tier_migration().unwrap();

    assert_eq!(engine.stats().cold_sstable_count, 1);
    assert_eq!(engine.stats().sstable_count, 0);
    assert_eq!(engine.get(b"user:101").unwrap(), Some(b"alice".to_vec()));

    // 2. Delete user:101 in Hot Tier (writes tombstone) and flush
    engine.delete(b"user:101").unwrap();
    engine.flush().unwrap();

    // 3. Write user:102 in Hot Tier and flush
    engine.put(b"user:102", b"bob").unwrap();
    engine.flush().unwrap();

    assert_eq!(engine.stats().sstable_count, 2, "Hot tier must have 2 SSTables");

    // 4. Trigger Hot Compaction
    let compacted_count = engine.compact().unwrap();
    assert_eq!(compacted_count, 2, "Must compact the 2 hot SSTables");
    assert_eq!(engine.stats().sstable_count, 1, "Hot tier now has 1 merged SSTable");

    // 5. CRITICAL VERIFICATION: Tombstone MUST be retained in hot merged table
    // to shadow the old record in Cold tier and prevent zombie resurrection!
    let deleted_user = engine.get(b"user:101").unwrap();
    assert_eq!(
        deleted_user, None,
        "Zombie resurrection bug: user:101 was deleted but reappeared from cold storage!"
    );

    let active_user = engine.get(b"user:102").unwrap();
    assert_eq!(active_user, Some(b"bob".to_vec()));

    let scan_results = engine.prefix_scan(b"user:").unwrap();
    assert_eq!(scan_results.len(), 1);
    assert_eq!(scan_results[0], (b"user:102".to_vec(), b"bob".to_vec()));
}

#[test]
fn test_cold_sstable_compaction_and_purging() {
    let dir = tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let hot_dir = data_dir.join("sst");
    let cold_dir = data_dir.join("cold");

    let tiered_cfg = TieredStorageConfig {
        hot_dir: hot_dir.clone(),
        cold_dir: Some(cold_dir.clone()),
        max_hot_bytes: 1_000_000,
        cold_migration_age_days: 0,
        enable_auto_tiering: false,
    };

    let storage_cfg = StorageConfig {
        data_dir: data_dir.clone(),
        memtable_size: 10_000,
        tiered_storage: Some(tiered_cfg),
        ..Default::default()
    };

    let engine = StorageEngine::open(storage_cfg).unwrap();

    // Batch 1 -> Cold
    engine.put(b"archive:item1", b"v1_old").unwrap();
    engine.flush().unwrap();
    engine.trigger_tier_migration().unwrap();

    // Batch 2 -> Cold (updates item1, adds item2)
    engine.put(b"archive:item1", b"v1_updated").unwrap();
    engine.put(b"archive:item2", b"v2_created").unwrap();
    engine.flush().unwrap();
    engine.trigger_tier_migration().unwrap();

    // Batch 3 -> Cold (adds item3, deletes item2)
    engine.put(b"archive:item3", b"v3_created").unwrap();
    engine.delete(b"archive:item2").unwrap();
    engine.flush().unwrap();
    engine.trigger_tier_migration().unwrap();

    assert_eq!(engine.stats().cold_sstable_count, 3, "Should have 3 cold SSTables");

    // Compact Cold tier
    let cold_compacted = engine.compact_cold().unwrap();
    assert_eq!(cold_compacted, 3, "All 3 cold SSTables should be merged");
    assert_eq!(engine.stats().cold_sstable_count, 1, "Cold tier should now have 1 compacted table");

    // Verify point lookups and prefix scans on compacted cold tier
    assert_eq!(engine.get(b"archive:item1").unwrap(), Some(b"v1_updated".to_vec()));
    assert_eq!(engine.get(b"archive:item2").unwrap(), None, "Deleted item2 should be tombstoned/purged");
    assert_eq!(engine.get(b"archive:item3").unwrap(), Some(b"v3_created".to_vec()));

    let scan = engine.prefix_scan(b"archive:").unwrap();
    assert_eq!(scan.len(), 2);
    assert_eq!(scan[0], (b"archive:item1".to_vec(), b"v1_updated".to_vec()));
    assert_eq!(scan[1], (b"archive:item3".to_vec(), b"v3_created".to_vec()));
}


