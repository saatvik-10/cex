import type { Context } from "hono";
import { Hono } from "hono";
import { SUPPORTED_ASSETS } from "../lib/amount";
import type { AppContext } from "../utils/context";
import {
  createToken,
  TOKEN_KIND_ACCESS,
  TOKEN_KIND_REFRESH,
  verifyToken,
} from "../lib/jwt";
import { hashPassword, sha256Hex, verifyPassword } from "../lib/password";
import { authUser } from "../middleware/require-auth";
import { creditDemoBasket } from "../lib/demo";
import {
  refreshValidator,
  signinValidator,
  signupValidator,
} from "../validators/auth.validator";
import { firstError } from "../validators/helpers";

type User = { id: string; username: string; isDemo: boolean };

async function issueTokens(ctx: AppContext, user: User) {
  const access_token = await createToken(
    user.id,
    TOKEN_KIND_ACCESS,
    ctx.config.jwtSecret,
    ctx.config.accessTtlSeconds,
  );
  const refresh_token = await createToken(
    user.id,
    TOKEN_KIND_REFRESH,
    ctx.config.jwtSecret,
    ctx.config.refreshTtlSeconds,
  );
  await ctx.db.refreshToken.create({
    data: {
      userId: user.id,
      tokenHash: sha256Hex(refresh_token),
      expiresAt: new Date(Date.now() + ctx.config.refreshTtlSeconds * 1000),
    },
  });
  return {
    access_token,
    refresh_token,
    user: { id: user.id, username: user.username, isDemo: user.isDemo },
  };
}

async function parseBody(
  c: Context<{ Variables: { ctx: AppContext } }>,
): Promise<Record<string, unknown> | null> {
  try {
    const body = await c.req.json();
    return body !== null && typeof body === "object"
      ? (body as Record<string, unknown>)
      : null;
  } catch {
    return null;
  }
}

export const authRoutes = new Hono<{ Variables: { ctx: AppContext } }>();

authRoutes.post("/signup", async (c) => {
  const ctx = c.get("ctx");
  const parsed = signupValidator.safeParse(await parseBody(c));
  if (!parsed.success) {
    return c.json({ error: firstError(parsed.error) }, 400);
  }
  const { username, password } = parsed.data;

  const passwordHash = await hashPassword(password);
  let user: User;
  try {
    user = await ctx.db.$transaction(async (tx) => {
      const created = await tx.user.create({
        data: { username, passwordHash },
      });
      await tx.balance.createMany({
        data: SUPPORTED_ASSETS.map((asset) => ({
          userId: created.id,
          asset,
          amount: 0,
        })),
      });
      return { id: created.id, username: created.username, isDemo: created.isDemo };
    });
  } catch (e) {
    if ((e as { code?: string }).code === "P2002") {
      return c.json({ error: "username already taken" }, 409);
    }
    throw e;
  }

  const tokens = await issueTokens(ctx, user);

  // Demo practice account: seed the practice basket through the engine. Best
  // effort — if the engine is momentarily unavailable the account still
  // exists and the user can top up later via the faucet.
  const seed = await creditDemoBasket(ctx, user.id, `demo-seed:${user.id}`).catch(
    () => ({ ok: false as const, error: "seed failed" }),
  );
  if (!seed.ok) {
    console.error(`signup: demo seed failed for ${user.id}: ${seed.error}`);
  }

  return c.json(tokens, 200);
});

authRoutes.post("/signin", async (c) => {
  const ctx = c.get("ctx");
  const parsed = signinValidator.safeParse(await parseBody(c));
  if (!parsed.success) {
    return c.json({ error: "unauthorized" }, 401);
  }
  const { username, password } = parsed.data;

  const user = await ctx.db.user.findFirst({ where: { username } });
  if (user === null || !(await verifyPassword(password, user.passwordHash))) {
    return c.json({ error: "unauthorized" }, 401);
  }

  return c.json(
    await issueTokens(ctx, {
      id: user.id,
      username: user.username,
      isDemo: user.isDemo,
    }),
    200,
  );
});

authRoutes.post("/refresh", async (c) => {
  const ctx = c.get("ctx");
  const parsed = refreshValidator.safeParse(await parseBody(c));
  if (!parsed.success) {
    return c.json({ error: "unauthorized" }, 401);
  }
  const refreshToken = parsed.data.refresh_token;

  const verified = await verifyToken(refreshToken, ctx.config.jwtSecret);
  if (verified === null || verified.kind !== TOKEN_KIND_REFRESH) {
    return c.json({ error: "unauthorized" }, 401);
  }

  const tokenHash = sha256Hex(refreshToken);
  const row = await ctx.db.refreshToken.findFirst({
    where: { tokenHash, revoked: false, expiresAt: { gt: new Date() } },
  });
  if (row === null || row.userId !== verified.userId) {
    return c.json({ error: "unauthorized" }, 401);
  }

  await ctx.db.refreshToken.update({
    where: { id: row.id },
    data: { revoked: true },
  });

  const user = await ctx.db.user.findUnique({ where: { id: verified.userId } });
  if (user === null) {
    return c.json({ error: "unauthorized" }, 401);
  }

  return c.json(
    await issueTokens(ctx, {
      id: user.id,
      username: user.username,
      isDemo: user.isDemo,
    }),
    200,
  );
});

authRoutes.get("/profile", async (c) => {
  const ctx = c.get("ctx");
  const auth = await authUser(c, ctx);
  if (!auth.ok) return auth.res;

  const user = await ctx.db.user.findUnique({ where: { id: auth.userId } });
  if (user === null) {
    return c.json({ error: "not found" }, 404);
  }
  return c.json(
    { id: user.id, username: user.username, isDemo: user.isDemo },
    200,
  );
});
