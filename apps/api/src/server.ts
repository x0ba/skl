import { serve } from "@hono/node-server";
import { app } from "./app";
import { migrate } from "./db/migrate";
import { env } from "./env";

await migrate();

serve({ fetch: app.fetch, port: env.PORT }, (info) => {
  console.log(`skl api listening on http://localhost:${info.port}`);
  console.log(`health: http://localhost:${info.port}/v1/health`);
});
