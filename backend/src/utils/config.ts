export type Config = {
  databaseUrl: string;
  redisUrl: string;
  jwtSecret: string;
  accessTtlSeconds: number;
  refreshTtlSeconds: number;
  port: number;
  replyTimeoutMs: number;
};

function intFromEnv(key: string, fallback: number): number {
  const raw = process.env[key];
  if (raw === undefined) return fallback;
  const parsed = Number.parseInt(raw, 10);
  return Number.isFinite(parsed) ? parsed : fallback;
}

export function loadConfig(): Config {
  const missing = ["DATABASE_URL", "REDIS_URL", "JWT_SECRET"].filter(
    (k) => process.env[k] === undefined,
  );
  if (missing.length > 0) {
    throw new Error(`missing required env var(s): ${missing.join(", ")}`);
  }

  const accessMinutes = intFromEnv("ACCESS_TTL_MINUTES", 15);
  const refreshDays = intFromEnv("REFRESH_TTL_DAYS", 7);

  return {
    databaseUrl: process.env.DATABASE_URL!,
    redisUrl: process.env.REDIS_URL!,
    jwtSecret: process.env.JWT_SECRET!,
    accessTtlSeconds: accessMinutes * 60,
    refreshTtlSeconds: refreshDays * 24 * 60 * 60,
    port: intFromEnv("PORT", 8000),
    replyTimeoutMs: intFromEnv("REPLY_TIMEOUT_MS", 2000),
  };
}
