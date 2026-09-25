import { z } from "zod";
import { SUPPORTED_ASSETS } from "../lib/amount";

/**
 * A money value as it arrives on the wire. Mirrors the legacy API (which
 * deserialized via BigDecimal): accepts plain JSON numbers as well as strings,
 * allows `.5` / `5.` spellings, requires a positive result and at most 18
 * fractional digits. Normalizes to the canonical string form the engine sees.
 */
export const amountValidator = z
  .union([z.number().finite(), z.string()], { error: "amount is required" })
  .transform((value) => (typeof value === "number" ? String(value) : value))
  .pipe(
    z
      .string()
      .regex(
        /^(?:\d+(?:\.\d{1,18})?|\.\d{1,18}|\d+\.)$/,
        "amount must be a positive decimal with at most 18 fractional digits",
      )
      .refine((text) => /[1-9]/.test(text.replace(/\./g, "")), {
        message: "amount must be positive",
      }),
  );

export const depositBodyValidator = z.object({
  amount: amountValidator,
  idempotency_key: z.string().min(1).optional(),
});

// Case-insensitive asset matching, e.g. `/deposit/sol` == `SOL`.
export const assetParamValidator = z.preprocess(
  (raw) => (typeof raw === "string" ? raw.toUpperCase() : raw),
  z.enum(SUPPORTED_ASSETS),
);

export type DepositBody = z.infer<typeof depositBodyValidator>;
export type SupportedAsset = z.infer<typeof assetParamValidator>;