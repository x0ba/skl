import { Hono } from "hono";
import { cors } from "hono/cors";
import { logger } from "hono/logger";
import { API_ROUTES } from "./contracts";
import { corsOrigins } from "./env";
import { authDeviceRoutes } from "./routes/auth-device";
import { blobRoutes } from "./routes/blobs";
import { deviceRoutes } from "./routes/devices";
import { healthRoutes } from "./routes/health";
import { skillRoutes } from "./routes/skills";
import { syncRoutes } from "./routes/sync";

export const app = new Hono()
  .use("*", logger())
  .use(
    "*",
    cors({
      origin: corsOrigins(),
      allowHeaders: ["Authorization", "Content-Type"],
      allowMethods: ["GET", "POST", "PUT", "DELETE", "OPTIONS"],
    }),
  )
  .get("/", (c) =>
    c.json({
      name: "skl-api",
      health: API_ROUTES.health,
    }),
  )
  .route("/", healthRoutes)
  .route("/", authDeviceRoutes)
  .route("/", deviceRoutes)
  .route("/", blobRoutes)
  .route("/", skillRoutes)
  .route("/", syncRoutes);

export type AppType = typeof app;
