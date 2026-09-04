CREATE TABLE IF NOT EXISTS users (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  clerk_user_id text NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX IF NOT EXISTS users_clerk_user_id_idx ON users (clerk_user_id);

CREATE TABLE IF NOT EXISTS devices (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
  name text NOT NULL,
  token_hash text,
  created_at timestamptz NOT NULL DEFAULT now(),
  last_used_at timestamptz,
  revoked_at timestamptz
);

CREATE UNIQUE INDEX IF NOT EXISTS devices_token_hash_idx ON devices (token_hash);
CREATE INDEX IF NOT EXISTS devices_user_id_idx ON devices (user_id);

CREATE TABLE IF NOT EXISTS device_authorizations (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  device_code_hash text NOT NULL,
  user_code text NOT NULL,
  user_code_normalized text NOT NULL,
  device_name text NOT NULL DEFAULT 'cli',
  interval_seconds integer NOT NULL DEFAULT 5,
  poll_interval_seconds integer NOT NULL DEFAULT 5,
  last_polled_at timestamptz,
  expires_at timestamptz NOT NULL,
  approved_at timestamptz,
  denied_at timestamptz,
  user_id uuid REFERENCES users (id) ON DELETE CASCADE,
  device_id uuid REFERENCES devices (id) ON DELETE SET NULL,
  token_issued_at timestamptz,
  created_at timestamptz NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX IF NOT EXISTS device_authorizations_device_code_hash_idx
  ON device_authorizations (device_code_hash);
CREATE UNIQUE INDEX IF NOT EXISTS device_authorizations_user_code_normalized_idx
  ON device_authorizations (user_code_normalized);

CREATE TABLE IF NOT EXISTS blobs (
  hash text PRIMARY KEY,
  content bytea NOT NULL,
  size_bytes integer NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS skills (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
  name text NOT NULL,
  metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
  current_version_id uuid,
  current_tree_hash text,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX IF NOT EXISTS skills_user_id_name_idx ON skills (user_id, name);
CREATE INDEX IF NOT EXISTS skills_user_id_idx ON skills (user_id);

CREATE TABLE IF NOT EXISTS skill_versions (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  skill_id uuid NOT NULL REFERENCES skills (id) ON DELETE CASCADE,
  tree_hash text NOT NULL,
  created_by_device_id uuid REFERENCES devices (id) ON DELETE SET NULL,
  metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
  created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS skill_versions_skill_id_idx ON skill_versions (skill_id);
CREATE INDEX IF NOT EXISTS skill_versions_tree_hash_idx ON skill_versions (tree_hash);

CREATE TABLE IF NOT EXISTS skill_files (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  version_id uuid NOT NULL REFERENCES skill_versions (id) ON DELETE CASCADE,
  path text NOT NULL,
  content_hash text NOT NULL REFERENCES blobs (hash)
);

CREATE UNIQUE INDEX IF NOT EXISTS skill_files_version_id_path_idx
  ON skill_files (version_id, path);
CREATE INDEX IF NOT EXISTS skill_files_content_hash_idx ON skill_files (content_hash);
