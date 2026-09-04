import { eq } from "drizzle-orm";
import { Hono } from "hono";
import type { PutBlobResponse } from "../contracts";
import { db } from "../db";
import { blobs } from "../db/schema";
import type { AuthVariables } from "../lib/auth";
import { requireAuth } from "../lib/auth";
import { isSha256Hex, normalizeHash, sha256Hex } from "../lib/hash";
import { jsonError } from "../lib/http";

const MAX_BLOB_BYTES = 8 * 1024 * 1024;

export const blobRoutes = new Hono<{ Variables: AuthVariables }>();

blobRoutes.use("/blobs/:hash", requireAuth);

blobRoutes.put("/blobs/:hash", async (c) => {
  const hash = normalizeHash(c.req.param("hash"));
  if (!isSha256Hex(hash)) {
    return jsonError(c, 400, "invalid_hash", "Expected lowercase hex SHA-256");
  }

  const raw = new Uint8Array(await c.req.arrayBuffer());
  if (raw.byteLength > MAX_BLOB_BYTES) {
    return jsonError(c, 413, "blob_too_large", `Max ${MAX_BLOB_BYTES} bytes`);
  }

  const computed = sha256Hex(raw);
  if (computed !== hash) {
    return jsonError(c, 400, "hash_mismatch", "Body SHA-256 does not match :hash");
  }

  const existing = await db.select({ hash: blobs.hash }).from(blobs).where(eq(blobs.hash, hash)).limit(1);
  if (!existing[0]) {
    await db.insert(blobs).values({
      hash,
      content: raw,
      sizeBytes: raw.byteLength,
    });
  }

  const body: PutBlobResponse = { hash, size: raw.byteLength };
  return c.json(body, existing[0] ? 200 : 201);
});

blobRoutes.get("/blobs/:hash", async (c) => {
  const hash = normalizeHash(c.req.param("hash"));
  if (!isSha256Hex(hash)) {
    return jsonError(c, 400, "invalid_hash", "Expected lowercase hex SHA-256");
  }

  const rows = await db.select().from(blobs).where(eq(blobs.hash, hash)).limit(1);
  const blob = rows[0];
  if (!blob) {
    return jsonError(c, 404, "blob_not_found");
  }

  return new Response(Buffer.from(blob.content), {
    status: 200,
    headers: {
      "content-type": "application/octet-stream",
      "content-length": String(blob.sizeBytes),
      "x-content-hash": blob.hash,
    },
  });
});
