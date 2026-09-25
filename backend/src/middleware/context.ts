import type { Context, Next } from "hono";
import type { AppContext } from "../utils/context";

/**
 * App-level middleware: attach the singleton AppContext (config, db, queue) to
 * every request so handlers and auth middleware can read `c.get("ctx")`.
 */
export function withAppContext(ctx: AppContext) {
  return async (c: Context<{ Variables: { ctx: AppContext } }>, next: Next) => {
    c.set("ctx", ctx);
    await next();
  };
}