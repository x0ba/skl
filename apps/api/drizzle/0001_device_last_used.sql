ALTER TABLE devices ADD COLUMN IF NOT EXISTS last_used_at timestamptz;
ALTER TABLE devices ALTER COLUMN token_hash DROP NOT NULL;
