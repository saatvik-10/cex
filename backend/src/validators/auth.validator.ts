import { z } from "zod";

export const signupValidator = z.object({
  // Stored exactly as sent; only the non-blank check is done on the trimmed
  // form (matches the hand-rolled behaviour before zod).
  username: z
    .string({ error: "username is required" })
    .refine((s) => s.trim().length > 0, "username is required"),
  password: z
    .string({ error: "password must be at least 8 characters" })
    .min(8, "password must be at least 8 characters"),
});

export const signinValidator = z.object({
  username: z.string(),
  password: z.string(),
});

export const refreshValidator = z.object({
  refresh_token: z.string().min(1),
});

export type SignupInput = z.infer<typeof signupValidator>;
export type SigninInput = z.infer<typeof signinValidator>;
export type RefreshInput = z.infer<typeof refreshValidator>;