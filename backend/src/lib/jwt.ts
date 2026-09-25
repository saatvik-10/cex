import { SignJWT, jwtVerify } from "jose";

export const TOKEN_KIND_ACCESS = "access";
export const TOKEN_KIND_REFRESH = "refresh";

export type VerifiedToken = {
  userId: string;
  kind: string;
};

export async function createToken(
  userId: string,
  kind: string,
  secret: string,
  ttlSeconds: number,
): Promise<string> {
  const now = Math.floor(Date.now() / 1000);
  return new SignJWT({ typ: kind })
    .setProtectedHeader({ alg: "HS256" })
    .setSubject(userId)
    .setJti(crypto.randomUUID())
    .setIssuedAt(now)
    .setExpirationTime(now + ttlSeconds)
    .sign(new TextEncoder().encode(secret));
}

export async function verifyToken(
  token: string,
  secret: string,
): Promise<VerifiedToken | null> {
  try {
    const { payload } = await jwtVerify(
      token,
      new TextEncoder().encode(secret),
    );
    if (typeof payload.sub !== "string" || typeof payload.typ !== "string") {
      return null;
    }
    return { userId: payload.sub, kind: payload.typ };
  } catch {
    return null;
  }
}
