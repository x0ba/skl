import { relations } from "drizzle-orm";
import {
  customType,
  index,
  integer,
  jsonb,
  pgTable,
  text,
  timestamp,
  uniqueIndex,
  uuid,
} from "drizzle-orm/pg-core";

/** Opaque ciphertext-ready payload. M0 stores raw bytes in Postgres. */
const bytea = customType<{ data: Uint8Array; driverData: Uint8Array }>({
  dataType() {
    return "bytea";
  },
  toDriver(value: Uint8Array): Uint8Array {
    return value;
  },
  fromDriver(value: unknown): Uint8Array {
    if (value instanceof Uint8Array) {
      return value;
    }
    throw new Error("Invalid bytea value");
  },
});

export const users = pgTable("users", {
  id: uuid("id").defaultRandom().primaryKey(),
  clerkUserId: text("clerk_user_id").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true })
    .notNull()
    .defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true })
    .notNull()
    .defaultNow(),
}, (table) => [
  uniqueIndex("users_clerk_user_id_idx").on(table.clerkUserId),
]);

export const devices = pgTable("devices", {
  id: uuid("id").defaultRandom().primaryKey(),
  userId: uuid("user_id")
    .notNull()
    .references(() => users.id, { onDelete: "cascade" }),
  name: text("name").notNull(),
  tokenHash: text("token_hash").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true })
    .notNull()
    .defaultNow(),
  revokedAt: timestamp("revoked_at", { withTimezone: true }),
}, (table) => [
  uniqueIndex("devices_token_hash_idx").on(table.tokenHash),
  index("devices_user_id_idx").on(table.userId),
]);

export const deviceAuthorizations = pgTable("device_authorizations", {
  id: uuid("id").defaultRandom().primaryKey(),
  deviceCodeHash: text("device_code_hash").notNull(),
  userCode: text("user_code").notNull(),
  userCodeNormalized: text("user_code_normalized").notNull(),
  deviceName: text("device_name").notNull().default("cli"),
  intervalSeconds: integer("interval_seconds").notNull().default(5),
  pollIntervalSeconds: integer("poll_interval_seconds").notNull().default(5),
  lastPolledAt: timestamp("last_polled_at", { withTimezone: true }),
  expiresAt: timestamp("expires_at", { withTimezone: true }).notNull(),
  approvedAt: timestamp("approved_at", { withTimezone: true }),
  deniedAt: timestamp("denied_at", { withTimezone: true }),
  userId: uuid("user_id").references(() => users.id, { onDelete: "cascade" }),
  deviceId: uuid("device_id").references(() => devices.id, {
    onDelete: "set null",
  }),
  tokenIssuedAt: timestamp("token_issued_at", { withTimezone: true }),
  createdAt: timestamp("created_at", { withTimezone: true })
    .notNull()
    .defaultNow(),
}, (table) => [
  uniqueIndex("device_authorizations_device_code_hash_idx").on(
    table.deviceCodeHash,
  ),
  uniqueIndex("device_authorizations_user_code_normalized_idx").on(
    table.userCodeNormalized,
  ),
]);

export const blobs = pgTable("blobs", {
  hash: text("hash").primaryKey(),
  content: bytea("content").notNull(),
  sizeBytes: integer("size_bytes").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true })
    .notNull()
    .defaultNow(),
});

export const skills = pgTable("skills", {
  id: uuid("id").defaultRandom().primaryKey(),
  userId: uuid("user_id")
    .notNull()
    .references(() => users.id, { onDelete: "cascade" }),
  name: text("name").notNull(),
  metadata: jsonb("metadata").$type<Record<string, unknown>>().notNull().default({}),
  currentVersionId: uuid("current_version_id"),
  currentTreeHash: text("current_tree_hash"),
  createdAt: timestamp("created_at", { withTimezone: true })
    .notNull()
    .defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true })
    .notNull()
    .defaultNow(),
}, (table) => [
  uniqueIndex("skills_user_id_name_idx").on(table.userId, table.name),
  index("skills_user_id_idx").on(table.userId),
]);

export const skillVersions = pgTable("skill_versions", {
  id: uuid("id").defaultRandom().primaryKey(),
  skillId: uuid("skill_id")
    .notNull()
    .references(() => skills.id, { onDelete: "cascade" }),
  treeHash: text("tree_hash").notNull(),
  createdByDeviceId: uuid("created_by_device_id").references(() => devices.id, {
    onDelete: "set null",
  }),
  metadata: jsonb("metadata").$type<Record<string, unknown>>().notNull().default({}),
  createdAt: timestamp("created_at", { withTimezone: true })
    .notNull()
    .defaultNow(),
}, (table) => [
  index("skill_versions_skill_id_idx").on(table.skillId),
  index("skill_versions_tree_hash_idx").on(table.treeHash),
]);

export const skillFiles = pgTable("skill_files", {
  id: uuid("id").defaultRandom().primaryKey(),
  versionId: uuid("version_id")
    .notNull()
    .references(() => skillVersions.id, { onDelete: "cascade" }),
  path: text("path").notNull(),
  contentHash: text("content_hash")
    .notNull()
    .references(() => blobs.hash),
}, (table) => [
  uniqueIndex("skill_files_version_id_path_idx").on(table.versionId, table.path),
  index("skill_files_content_hash_idx").on(table.contentHash),
]);

export const usersRelations = relations(users, ({ many }) => ({
  devices: many(devices),
  skills: many(skills),
}));

export const devicesRelations = relations(devices, ({ one }) => ({
  user: one(users, { fields: [devices.userId], references: [users.id] }),
}));

export const skillsRelations = relations(skills, ({ one, many }) => ({
  user: one(users, { fields: [skills.userId], references: [users.id] }),
  versions: many(skillVersions),
}));

export const skillVersionsRelations = relations(skillVersions, ({ one, many }) => ({
  skill: one(skills, { fields: [skillVersions.skillId], references: [skills.id] }),
  files: many(skillFiles),
}));

export const skillFilesRelations = relations(skillFiles, ({ one }) => ({
  version: one(skillVersions, {
    fields: [skillFiles.versionId],
    references: [skillVersions.id],
  }),
  blob: one(blobs, {
    fields: [skillFiles.contentHash],
    references: [blobs.hash],
  }),
}));
