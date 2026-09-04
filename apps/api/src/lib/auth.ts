import { verifyToken } from "@clerk/backend";
import { and, eq, isNull } from "drizzle-orm";
import type { Context, Next } from "hono";
import { createMiddleware } from "hono/factory";
import { DEVICE_TOKEN_PREFIX, DEV_AUTH_PREFIX } from "../contracts";
import { db } from "../db";
import { devices, users } from "../db/schema";
import { env } from "../env";
import { hashToken } from "./hash";
import { jsonError } from "./http";
import { upsertUserByClerkId } from "./users";

export type AuthContext = {
  userId: string;
  clerkUserId: string;
  deviceId?: string;
};

export type AuthVariables = {
  auth: AuthContext;
};

type AppEnv = { Variables: AuthVariables };

export class AuthError extends Error {
  readonly status: 401 | 403;

  constructor(message: string, status: 401 | 403 = 401) {
    super(message);
    this.name = "AuthError";
    this.status = status;
  }
}

function bearerToken(c: Context): string | undefined {
  const header = c.req.header("authorization");
  if (!header) {
    return undefined;
  }
  const match = /^Bearer\s+(.+)$/i.exec(header);
  return match?.[1]?.trim();
}

async function resolveDeviceAuth(token: string): Promise<AuthContext> {
  const tokenHash = hashToken(token);
  const rows = await db
    .select({
      deviceId: devices.id,
      userId: devices.userId,
      clerkUserId: users.clerkUserId,
    })
    .from(devices)
    .innerJoin(users, eq(users.id, devices.userId))
    .where(and(eq(devices.tokenHash, tokenHash), isNull(devices.revokedAt)))
    .limit(1);

  const row = rows[0];
  if (!row) {
    throw new AuthError("invalid_token");
  }
  return {
    userId: row.userId,
    clerkUserId: row.clerkUserId,
    deviceId: row.deviceId,
  };
}

async function resolveClerkAuth(token: string): Promise<AuthContext> {
  if (env.ALLOW_DEV_AUTH && token.startsWith(DEV_AUTH_PREFIX)) {
    const clerkUserId = token.slice(DEV_AUTH_PREFIX.length).trim();
    if (!clerkUserId) {
      throw new AuthError("invalid_dev_token");
    }
    const user = await upsertUserByClerkId(clerkUserId);
    return { userId: user.id, clerkUserId: user.clerkUserId };
  }

  if (!env.CLERK_SECRET_KEY) {
    throw new AuthError("clerk_not_configured");
  }

  const payload = await verifyToken(token, {
    secretKey: env.CLERK_SECRET_KEY,
  });
  const clerkUserId = payload.sub;
  if (!clerkUserId) {
    throw new AuthError("invalid_clerk_token");
  }
  const user = await upsertUserByClerkId(clerkUserId);
  return { userId: user.id, clerkUserId: user.clerkUserId };
}

export async function authenticate(c: Context): Promise<AuthContext> {
  const token = bearerToken(c);
  if (!token) {
    throw new AuthError("missing_authorization");
  }
  if (token.startsWith(DEVICE_TOKEN_PREFIX)) {
    return resolveDeviceAuth(token);
  }
  return resolveClerkAuth(token);
}

export async function authenticateClerk(c: Context): Promise<AuthContext> {
  const token = bearerToken(c);
  if (!token) {
    throw new AuthError("missing_authorization");
  }
  if (token.startsWith(DEVICE_TOKEN_PREFIX)) {
    throw new AuthError("clerk_required", 403);
  }
  return resolveClerkAuth(token);
}

function authMiddleware(
  resolver: (c: Context) => Promise<AuthContext>,
) {
  return createMiddleware<AppEnv>(async (c, next: Next) => {
    try {
      const auth = await resolver(c);
      c.set("auth", auth);
      await next();
      return;
    } catch (error) {
      if (error instanceof AuthError) {
        return jsonError(c, error.status, error.message);
      }
      const message = error instanceof Error ? error.message : "unauthorized";
      if (message.toLowerCase().includes("expired") || message.toLowerCase().includes("jwt")) {
        return jsonError(c, 401, "invalid_clerk_token", message);
      }
      throw error;
    }
  });
}

export const requireAuth = authMiddleware(authenticate);
export const requireClerk = authMiddleware(authenticateClerk);

export function getAuth(c: Context<AppEnv>): AuthContext {
  return c.get("auth");
}
