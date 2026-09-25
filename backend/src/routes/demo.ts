import { Hono } from "hono";
import type { AppContext } from "../utils/context";
import { EngineQueue } from "../queue/queue";
import { authUser } from "../middleware/require-auth";
import {
  DEMO_BASKET_USD_SCALED,
  DEMO_CAP_SCALED,
  creditDemoBasket,
  usdEquivalent,
} from "../lib/demo";

export const demoRoutes = new Hono<{ Variables: { ctx: AppContext } }>();

/**
 * Demo-money faucet: top up a practice account with the standard demo basket,
 * rejected once the account's USD-equivalent balance is already at the cap.
 */
demoRoutes.post("/funds", async (c) => {
  const ctx = c.get("ctx");
  const auth = await authUser(c, ctx);
  if (!auth.ok) return auth.res;

  const balance = await ctx.queue.request(
    EngineQueue.newCommand("get_balances", crypto.randomUUID(), auth.userId),
  );
  if (balance.kind === "error") return c.json({ error: balance.error }, 503);
  if (balance.kind !== "balances_result")
    return c.json({ error: "unexpected engine result" }, 503);

  // Grant the basket only when the FULL basket fits under the cap, so no demo
  // account ever exceeds it.
  if (usdEquivalent(balance.balances) + DEMO_BASKET_USD_SCALED > DEMO_CAP_SCALED) {
    return c.json({ error: "demo balance cap reached" }, 400);
  }

  const grant = await creditDemoBasket(
    ctx,
    auth.userId,
    `demo-faucet:${crypto.randomUUID()}`,
  );
  if (!grant.ok) return c.json({ error: grant.error }, 503);

  return c.json({ credited: grant.credited }, 200);
});