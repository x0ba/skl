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

function parseOrigins(value: string): string[] {
  return value
    .split(",")
    .map((origin) => origin.trim().replace(/\/$/, ""))
    .filter((origin) => origin.length > 0);
}

/** Allow both apex and www so `tryskl.fyi` and `www.tryskl.fyi` both pass CORS. */
export function originVariants(origin: string): string[] {
  let parsed: URL;
  try {
    parsed = new URL(origin);
  } catch {
    return [origin];
  }
  const { protocol, hostname, port } = parsed;
  const suffix = port ? `:${port}` : "";
  const hosts = new Set<string>([hostname]);
  if (hostname.startsWith("www.")) {
    hosts.add(hostname.slice("www.".length));
  } else if (hostname !== "localhost" && hostname !== "127.0.0.1") {
    hosts.add(`www.${hostname}`);
  }
  return [...hosts].map((host) => `${protocol}//${host}${suffix}`);
}

export function corsOrigins(): string[] {
  const origins = new Set<string>([
    "http://localhost:3000",
    "http://127.0.0.1:3000",
  ]);
  for (const origin of parseOrigins(env.SKL_WEB_ORIGIN)) {
    for (const variant of originVariants(origin)) {
      origins.add(variant);
    }
  }
  return [...origins];
}
