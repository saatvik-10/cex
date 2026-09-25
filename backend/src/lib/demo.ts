import type { SUPPORTED_ASSETS } from "./amount";
import type { AppContext } from "../utils/context";
import { EngineQueue } from "../queue/queue";

// Demo-money configuration for the practice exchange. There is no real
// on-ramp; the basket and cap define how much pretend money a user can play
// with. Amounts are canonical engine strings (integer or 18-decimals).

/** Fixed demo portfolio credited at signup and by the faucet button. */
export const DEMO_BASKET: Record<(typeof SUPPORTED_ASSETS)[number], string> = {
  USD: "10000",
  SOL: "10",
  ETH: "5",
};

/** Reference prices used only to compute the USD-equivalent cap (demo math). */
export const REF_PRICES_USD: Record<(typeof SUPPORTED_ASSETS)[number], number> = {
  USD: 1,
  SOL: 150,
  ETH: 3000,
};

/** A demo account's total USD-equivalent balance may not exceed this. */
export const DEMO_CAP_USD = 100_000;

const SCALE = 10n ** 18n;

/** The cap expressed in the same BigInt (1e-18 scale) as usdEquivalent. */
export const DEMO_CAP_SCALED = BigInt(DEMO_CAP_USD) * SCALE;

/** USD-equivalent value of one full basket, same BigInt scale. */
export const DEMO_BASKET_USD_SCALED = Object.entries(DEMO_BASKET).reduce(
  (sum, [asset, amount]) => sum + usdValueOf(asset, amount),
  0n,
);

type BalanceLine = { asset: string; amount: string };

/**
 * Parse an engine amount string ("0", "5", "5.000000000000000000") to its
 * unscaled integer value in units of 1e-18. Exact BigInt math — 18-decimal
 * amounts far exceed Number.MAX_SAFE_INTEGER, so floats are not an option.
 */
export function amountToBigInt(text: string): bigint {
  const digits = text.startsWith(".")
    ? `0${text}`
    : text.endsWith(".")
      ? `${text}0`
      : text;
  const [whole, frac = ""] = digits.split(".");
  return (
    BigInt(
      whole === undefined || whole === "" || whole === "-" ? "0" : whole,
    ) * SCALE +
    (frac.length === 0 ? 0n : BigInt(frac.padEnd(18, "0")))
  );
}

/** USD-equivalent value of a single (asset, amount) pair, 1e-18 BigInt scale. */
export function usdValueOf(asset: string, amountText: string): bigint {
  const price = REF_PRICES_USD[asset as keyof typeof REF_PRICES_USD];
  if (price === undefined) return 0n;
  const value = amountToBigInt(amountText.trim());
  return value < 0n ? 0n : value * BigInt(Math.trunc(price));
}

/** Sum a user's balances into a USD-equivalent value (BigInt, 1e-18 scale). */
export function usdEquivalent(balances: BalanceLine[]): bigint {
  let total = 0n;
  for (const line of balances) total += usdValueOf(line.asset, line.amount);
  return total;
}

/** True when adding `extraUsdValue` would push the account over the demo cap. */
export function exceedsCap(
  currentUsd: bigint,
  extraUsdValue: bigint,
): boolean {
  return currentUsd + extraUsdValue > DEMO_CAP_SCALED;
}

export type DemoCreditOutcome =
  | { ok: true; credited: Record<string, string> }
  | { ok: false; error: string };

/**
 * Credit the demo basket through the standard engine deposit path (WAL,
 * idempotency keys and all). Keyed by `prefix:<asset>` so retries never
 * double-credit; callers pass a fresh prefix per logical grant.
 */
export async function creditDemoBasket(
  ctx: AppContext,
  userId: string,
  keyPrefix: string,
): Promise<DemoCreditOutcome> {
  const credited: Record<string, string> = {};
  for (const [asset, amount] of Object.entries(DEMO_BASKET)) {
    const outcome = await ctx.queue.request(
      EngineQueue.newCommand(
        "deposit",
        crypto.randomUUID(),
        userId,
        asset,
        amount,
        `${keyPrefix}:${asset}`,
      ),
    );
    if (outcome.kind === "error") return { ok: false, error: outcome.error };
    if (outcome.kind !== "deposit_result")
      return { ok: false, error: "unexpected engine result" };
    credited[outcome.asset] = outcome.amount;
  }
  return { ok: true, credited };
}