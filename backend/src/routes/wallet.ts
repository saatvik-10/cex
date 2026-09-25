import { Hono } from "hono";
import type { AppContext } from "../utils/context";
import { EngineQueue } from "../queue/queue";
import { authUser } from "../middleware/require-auth";
import {
  assetParamValidator,
  depositBodyValidator,
} from "../validators/wallet.validator";
import { firstError } from "../validators/helpers";
import { DEMO_CAP_SCALED, usdEquivalent, usdValueOf } from "../lib/demo";

export const walletRoutes = new Hono<{ Variables: { ctx: AppContext } }>();
export const depositRoutes = new Hono<{ Variables: { ctx: AppContext } }>();

walletRoutes.get("/balance", async (c) => {
  const ctx = c.get("ctx");
  const auth = await authUser(c, ctx);
  if (!auth.ok) return auth.res;

  const outcome = await ctx.queue.request(
    EngineQueue.newCommand("get_balances", crypto.randomUUID(), auth.userId),
  );
  if (outcome.kind === "error") return c.json({ error: outcome.error }, 503);
  if (outcome.kind !== "balances_result")
    return c.json({ error: "unexpected engine result" }, 503);

  return c.json({ balances: outcome.balances }, 200);
});

depositRoutes.post("/:asset", async (c) => {
  const ctx = c.get("ctx");
  const auth = await authUser(c, ctx);
  if (!auth.ok) return auth.res;

  const assetCheck = assetParamValidator.safeParse(c.req.param("asset"));
  if (!assetCheck.success) return c.json({ error: "unsupported asset" }, 400);
  const asset = assetCheck.data;

  let body: Record<string, unknown> | null;
  try {
    const parsed: unknown = await c.req.json();
    body =
      parsed !== null && typeof parsed === "object" && !Array.isArray(parsed)
        ? (parsed as Record<string, unknown>)
        : null;
  } catch {
    return c.json({ error: "invalid body" }, 400);
  }
  if (body === null) return c.json({ error: "amount is required" }, 400);

  const bodyCheck = depositBodyValidator.safeParse(body);
  if (!bodyCheck.success) return c.json({ error: firstError(bodyCheck.error) }, 400);
  const { amount, idempotency_key: clientKey } = bodyCheck.data;

  // Demo money is capped so an account can't print unlimited practice funds.
  // The engine is the balance authority, so read the current position first.
  const current = await ctx.queue.request(
    EngineQueue.newCommand("get_balances", crypto.randomUUID(), auth.userId),
  );
  if (current.kind === "error") return c.json({ error: current.error }, 503);
  if (current.kind !== "balances_result")
    return c.json({ error: "unexpected engine result" }, 503);
  if (usdEquivalent(current.balances) + usdValueOf(asset, amount) > DEMO_CAP_SCALED) {
    return c.json({ error: "demo balance cap exceeded" }, 400);
  }

  // Clients may supply their own idempotency key; otherwise generate one so a
  // redelivered command never double-credits.
  const idempotencyKey = clientKey ?? crypto.randomUUID();

  const outcome = await ctx.queue.request(
    EngineQueue.newCommand(
      "deposit",
      crypto.randomUUID(),
      auth.userId,
      asset,
      amount,
      idempotencyKey,
    ),
  );
  if (outcome.kind === "error") return c.json({ error: outcome.error }, 503);
  if (outcome.kind !== "deposit_result")
    return c.json({ error: "unexpected engine result" }, 503);

  return c.json({ asset: outcome.asset, amount: outcome.amount }, 200);
});

export type { EngineQueue };
