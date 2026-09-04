import { drizzle } from "drizzle-orm/postgres-js";
import postgres from "postgres";
import { env } from "../env";
import * as schema from "./schema";

const globalForDb = globalThis as unknown as {
  sklSql?: ReturnType<typeof postgres>;
};

export const sql =
  globalForDb.sklSql ??
  postgres(env.DATABASE_URL, {
    max: 10,
    idle_timeout: 20,
    connect_timeout: 10,
  });

if (process.env.NODE_ENV !== "production") {
  globalForDb.sklSql = sql;
}

export const db = drizzle(sql, { schema });

export type Database = typeof db;
