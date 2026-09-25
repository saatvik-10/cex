import { Hono } from "hono";
import { loadConfig } from "./utils/config";
import { createDb } from "./db/db";
import type { AppContext } from "./utils/context";
import { EngineQueue } from "./queue/queue";
import { authRoutes } from "./routes/auth";
import { depositRoutes, walletRoutes } from "./routes/wallet";
import { demoRoutes } from "./routes/demo";
import { withAppContext } from "./middleware/context";

const config = loadConfig();
const db = createDb(config.databaseUrl);
const queue = new EngineQueue(config.redisUrl, config.replyTimeoutMs);
const ctx: AppContext = { config, db, queue };

const app = new Hono<{ Variables: { ctx: AppContext } }>();
app.use("*", withAppContext(ctx));

app.route("/auth", authRoutes);
app.route("/wallet", walletRoutes);
app.route("/deposit", depositRoutes);
app.route("/demo", demoRoutes);

app.get("/health", (c) => c.json({ status: "ok" }, 200));

// Match the legacy API's JSON error envelope on every path, not just handlers.
app.notFound((c) => c.json({ error: "not found" }, 404));
app.onError((err, c) => {
  console.error(err);
  return c.json({ error: "internal error" }, 500);
});

Bun.serve({
  port: config.port,
  fetch: app.fetch,
});

console.log(`backend ready on :${config.port} (redis ${config.redisUrl})`);
