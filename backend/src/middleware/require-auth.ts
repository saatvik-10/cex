import type { Context } from "hono";
import type { AppContext } from "../utils/context";
import { TOKEN_KIND_ACCESS, verifyToken } from "../lib/jwt";

/**
 * Authenticate the request's `Authorization: Bearer <access>` token.
 * Returns the user id or a ready-to-send 401 response.
 */
export async function authUser(
  c: Context<{ Variables: { ctx: AppContext } }>,
  ctx: AppContext,
): Promise<{ ok: true; userId: string } | { ok: false; res: Response }> {
  const header = c.req.header("Authorization") ?? "";
  const token = header.startsWith("Bearer ")
    ? header.slice("Bearer ".length).trim()
    : "";
  if (token === "") {
    return { ok: false, res: c.json({ error: "unauthorized" }, 401) };
  }
  const verified = await verifyToken(token, ctx.config.jwtSecret);
  if (verified === null || verified.kind !== TOKEN_KIND_ACCESS) {
    return { ok: false, res: c.json({ error: "unauthorized" }, 401) };
  }
  return { ok: true, userId: verified.userId };
}