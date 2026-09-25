export function hashPassword(plain: string): Promise<string> {
  return Bun.password.hash(plain, { algorithm: "argon2id" });
}

export function verifyPassword(plain: string, hash: string): Promise<boolean> {
  return Bun.password.verify(plain, hash);
}

export function sha256Hex(value: string): string {
  return Bun.CryptoHasher.hash("sha256", value, "hex");
}
