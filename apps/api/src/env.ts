import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

function loadDotEnv(): void {
  const path = resolve(process.cwd(), ".env");
  if (!existsSync(path)) {
    return;
  }
  const text = readFileSync(path, "utf8");
  for (const line of text.split("\n")) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("#")) {
      continue;
    }
    const eq = trimmed.indexOf("=");
    if (eq <= 0) {
      continue;
    }
    const key = trimmed.slice(0, eq).trim();
    let value = trimmed.slice(eq + 1).trim();
    if (
      (value.startsWith('"') && value.endsWith('"')) ||
      (value.startsWith("'") && value.endsWith("'"))
    ) {
      value = value.slice(1, -1);
    }
    if (process.env[key] === undefined) {
      process.env[key] = value;
    }
  }
}

loadDotEnv();

function optional(name: string): string | undefined {
  const value = process.env[name];
  if (value === undefined || value.length === 0) {
    return undefined;
  }
  return value;
}

const clerkSecret = optional("CLERK_SECRET_KEY");
const allowDevAuthEnv = optional("ALLOW_DEV_AUTH");

export const env = {
  DATABASE_URL:
    optional("DATABASE_URL") ?? "postgres://skl:skl@localhost:5432/skl",
  PORT: Number(optional("PORT") ?? "8787"),
  CLERK_SECRET_KEY: clerkSecret,
  CLERK_PUBLISHABLE_KEY: optional("CLERK_PUBLISHABLE_KEY"),
  CLERK_WEBHOOK_SECRET: optional("CLERK_WEBHOOK_SECRET"),
  SKL_WEB_ORIGIN: optional("SKL_WEB_ORIGIN") ?? "http://localhost:3000",
  SKL_API_ORIGIN: optional("SKL_API_ORIGIN") ?? "http://localhost:8787",
  ALLOW_DEV_AUTH:
    allowDevAuthEnv === "true" || (allowDevAuthEnv !== "false" && !clerkSecret),
};

export function corsOrigins(): string[] {
  const origins = new Set<string>([
    env.SKL_WEB_ORIGIN,
    "http://localhost:3000",
    "http://127.0.0.1:3000",
  ]);
  return [...origins];
}
