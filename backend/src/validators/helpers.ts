import type { z } from "zod";

/** First issue's message, for the API's single-string `{ error }` envelope. */
export function firstError(error: z.ZodError): string {
  return error.issues[0]?.message ?? "invalid request";
}