import { zValidator } from "@hono/zod-validator";
import { eq } from "drizzle-orm";
import { Hono } from "hono";
import { z } from "zod";
import {
  DEVICE_TOKEN_PREFIX,
  type DeviceApproveResponse,
  type DeviceCodeResponse,
  type DeviceTokenError,
  type DeviceTokenSuccess,
} from "../contracts";
import { db } from "../db";
import { deviceAuthorizations, devices } from "../db/schema";
import { env } from "../env";
import { getAuth, requireClerk } from "../lib/auth";
import {
  DEFAULT_DEVICE_EXPIRES_IN,
  DEFAULT_DEVICE_INTERVAL,
  generateDeviceCode,
  generateUserCode,
  normalizeUserCode,
} from "../lib/codes";
import { hashToken, randomHex } from "../lib/hash";
import { jsonError } from "../lib/http";

const deviceCodeBody = z.object({
  device_name: z.string().trim().min(1).max(80).optional(),
});

const deviceTokenBody = z.object({
  device_code: z.string().min(1),
});

const deviceApproveBody = z.object({
  user_code: z.string().min(1),
  device_name: z.string().trim().min(1).max(80).optional(),
});

export const authDeviceRoutes = new Hono();

authDeviceRoutes.post(
  "/auth/device/code",
  zValidator("json", deviceCodeBody),
  async (c) => {
    const { device_name } = c.req.valid("json");
    const deviceCode = generateDeviceCode();
    const userCode = generateUserCode();
    const expiresAt = new Date(Date.now() + DEFAULT_DEVICE_EXPIRES_IN * 1000);

    await db.insert(deviceAuthorizations).values({
      deviceCodeHash: hashToken(deviceCode),
      userCode: userCode.display,
      userCodeNormalized: userCode.normalized,
      deviceName: device_name ?? "cli",
      intervalSeconds: DEFAULT_DEVICE_INTERVAL,
      pollIntervalSeconds: DEFAULT_DEVICE_INTERVAL,
      expiresAt,
    });

    const verificationUri = `${env.SKL_WEB_ORIGIN.replace(/\/$/, "")}/device`;
    const body: DeviceCodeResponse = {
      device_code: deviceCode,
      user_code: userCode.display,
      verification_uri: verificationUri,
      verification_uri_complete: `${verificationUri}?user_code=${userCode.display}`,
      expires_in: DEFAULT_DEVICE_EXPIRES_IN,
      interval: DEFAULT_DEVICE_INTERVAL,
    };
    return c.json(body, 200);
  },
);

authDeviceRoutes.post(
  "/auth/device/token",
  zValidator("json", deviceTokenBody),
  async (c) => {
    const { device_code } = c.req.valid("json");
    const now = new Date();

    const rows = await db
      .select()
      .from(deviceAuthorizations)
      .where(eq(deviceAuthorizations.deviceCodeHash, hashToken(device_code)))
      .limit(1);
    const authz = rows[0];

    if (!authz) {
      const body: DeviceTokenError = {
        error: "invalid_grant",
        error_description: "Unknown device_code",
      };
      return c.json(body, 400);
    }

    if (authz.expiresAt.getTime() <= now.getTime()) {
      const body: DeviceTokenError = {
        error: "expired_token",
        error_description: "device_code has expired",
      };
      return c.json(body, 400);
    }

    if (authz.deniedAt) {
      const body: DeviceTokenError = {
        error: "access_denied",
        error_description: "User denied the request",
      };
      return c.json(body, 400);
    }

    if (authz.tokenIssuedAt) {
      const body: DeviceTokenError = {
        error: "invalid_grant",
        error_description: "device_code already used",
      };
      return c.json(body, 400);
    }

    const approved = Boolean(authz.approvedAt && authz.userId);
    const minIntervalMs = authz.pollIntervalSeconds * 1000;
    if (
      !approved &&
      authz.lastPolledAt &&
      now.getTime() - authz.lastPolledAt.getTime() < minIntervalMs
    ) {
      await db
        .update(deviceAuthorizations)
        .set({
          lastPolledAt: now,
          pollIntervalSeconds: authz.pollIntervalSeconds + DEFAULT_DEVICE_INTERVAL,
        })
        .where(eq(deviceAuthorizations.id, authz.id));
      const body: DeviceTokenError = {
        error: "slow_down",
        error_description: "Polling too frequently",
      };
      return c.json(body, 400);
    }

    await db
      .update(deviceAuthorizations)
      .set({ lastPolledAt: now })
      .where(eq(deviceAuthorizations.id, authz.id));

    if (!approved || !authz.userId) {
      const body: DeviceTokenError = {
        error: "authorization_pending",
        error_description: "Waiting for user approval",
      };
      return c.json(body, 400);
    }

    const rawToken = `${DEVICE_TOKEN_PREFIX}${randomHex(32)}`;
    const [device] = await db
      .insert(devices)
      .values({
        userId: authz.userId,
        name: authz.deviceName,
        tokenHash: hashToken(rawToken),
      })
      .returning();

    if (!device) {
      return jsonError(c, 500, "device_create_failed");
    }

    await db
      .update(deviceAuthorizations)
      .set({
        tokenIssuedAt: now,
        deviceId: device.id,
      })
      .where(eq(deviceAuthorizations.id, authz.id));

    const body: DeviceTokenSuccess = {
      access_token: rawToken,
      token_type: "Bearer",
      device_id: device.id,
    };
    return c.json(body, 200);
  },
);

authDeviceRoutes.post(
  "/auth/device/approve",
  requireClerk,
  zValidator("json", deviceApproveBody),
  async (c) => {
    const auth = getAuth(c);
    const { user_code, device_name } = c.req.valid("json");
    const normalized = normalizeUserCode(user_code);
    const now = new Date();

    const rows = await db
      .select()
      .from(deviceAuthorizations)
      .where(eq(deviceAuthorizations.userCodeNormalized, normalized))
      .limit(1);
    const authz = rows[0];

    if (!authz) {
      return jsonError(c, 404, "unknown_user_code", "No pending device for that code");
    }
    if (authz.expiresAt.getTime() <= now.getTime()) {
      return jsonError(c, 400, "expired_token", "user_code has expired");
    }
    if (authz.approvedAt) {
      return jsonError(c, 409, "already_approved", "This code was already approved");
    }
    if (authz.deniedAt) {
      return jsonError(c, 400, "access_denied", "This code was denied");
    }

    const nextName = device_name ?? authz.deviceName;
    await db
      .update(deviceAuthorizations)
      .set({
        approvedAt: now,
        userId: auth.userId,
        deviceName: nextName,
      })
      .where(eq(deviceAuthorizations.id, authz.id));

    const body: DeviceApproveResponse = {
      approved: true,
      device_name: nextName,
    };
    return c.json(body, 200);
  },
);
