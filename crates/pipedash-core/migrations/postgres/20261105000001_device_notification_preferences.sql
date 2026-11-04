ALTER TABLE paired_devices ADD COLUMN IF NOT EXISTS notification_preferences TEXT;

ALTER TABLE device_alerts ADD COLUMN IF NOT EXISTS provider_ids TEXT;
