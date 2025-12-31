-- Remove S3 sync state table (no longer needed after S3/GCS removal)
-- This migration removes S3-specific synchronization tracking

DROP TABLE IF EXISTS s3_sync_state;

-- Note: storage_metadata and storage_migrations tables are preserved
-- as they are generic and can be used for tracking any storage backend migrations
