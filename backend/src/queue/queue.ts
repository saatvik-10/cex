import Redis from "ioredis";

export const COMMANDS_STREAM = "stream:commands";
export const RESULTS_STREAM = "stream:results";
const PAYLOAD = "payload";

export type Command =
  | { kind: "ping"; reply_key: string }
  | { kind: "get_balances"; reply_key: string; user_id: string }
  | {
      kind: "deposit";
      reply_key: string;
      user_id: string;
      asset: string;
      amount: string;
      idempotency_key?: string;
    };

export type Result =
  | { kind: "pong"; reply_key: string }
  | {
      kind: "balances_result";
      reply_key: string;
      balances: { asset: string; amount: string }[];
    }
  | { kind: "deposit_result"; reply_key: string; asset: string; amount: string }
  | { kind: "error"; reply_key: string; error: string };

/**
 * Request/reply correlation over Redis Streams.
 *
 * Each request gets a fresh `reply_key`; a background subscriber reads results
 * from `stream:results` and resolves the matching pending entry. The pending
 * map is pruned by a timeout, turning engine silence into a 503 upstream.
 */
export class EngineQueue {
  private pub: Redis;
  private pending = new Map<
    string,
    { resolve: (r: Result) => void; timer: ReturnType<typeof setTimeout> }
  >();
  private readonly replyTimeoutMs: number;

  constructor(redisUrl: string, replyTimeoutMs: number) {
    // No offline queue: when Redis is unreachable a publish fails NOW, so a
    // 503 to the client genuinely means "not accepted" (never a queued command
    // that lands later behind a response that looked like a failure).
    this.pub = new Redis(redisUrl, { enableOfflineQueue: false });
    // ioredis reconnects on its own; without a listener it logs raw ECONNREFUSED
    // noise to stderr every retry. Swallow it (request() surfaces the outage as
    // a 503 to clients instead).
    this.pub.on("error", () => {});
    this.replyTimeoutMs = replyTimeoutMs;
    void this.runSubscriber(redisUrl);
  }

  /** Publish one command and await the engine's matching result (or timeout). */
  async request(command: Command): Promise<Result> {
    const result = new Promise<Result>((resolve) => {
      const timer = setTimeout(() => {
        this.pending.delete(command.reply_key);
        resolve({
          kind: "error",
          reply_key: command.reply_key,
          error: "engine reply timeout",
        });
      }, this.replyTimeoutMs);
      this.pending.set(command.reply_key, { resolve, timer });
    });

    // Bound the publish itself: if Redis is unreachable, answering with a 503
    // right now beats hanging the HTTP request (or worse, the client doing it).
    const publish = this.pub
      .xadd(
        COMMANDS_STREAM,
        "MAXLEN",
        "~",
        1000,
        "*",
        PAYLOAD,
        JSON.stringify(command),
      )
      .then(() => result)
      .catch(() => this.fail(command.reply_key, "engine queue unavailable"));

    const timeout = new Promise<Result>((resolve) => {
      setTimeout(
        () => resolve(this.fail(command.reply_key, "engine queue unavailable")),
        this.replyTimeoutMs,
      );
    });

    return Promise.race([publish, timeout]);
  }

  private fail(replyKey: string, error: string): Result {
    const entry = this.pending.get(replyKey);
    if (entry !== undefined) {
      clearTimeout(entry.timer);
      this.pending.delete(replyKey);
    }
    return { kind: "error", reply_key: replyKey, error };
  }

  static newCommand(kind: "ping", replyKey: string): Command;
  static newCommand(
    kind: "get_balances",
    replyKey: string,
    userId: string,
  ): Command;
  static newCommand(
    kind: "deposit",
    replyKey: string,
    userId: string,
    asset: string,
    amount: string,
    idempotencyKey: string,
  ): Command;
  static newCommand(
    kind: Command["kind"],
    replyKey: string,
    ...rest: string[]
  ): Command {
    switch (kind) {
      case "ping":
        return { kind, reply_key: replyKey };
      case "get_balances":
        return { kind, reply_key: replyKey, user_id: rest[0]! };
      case "deposit":
        return {
          kind,
          reply_key: replyKey,
          user_id: rest[0]!,
          asset: rest[1]!,
          amount: rest[2]!,
          idempotency_key: rest[3],
        };
    }
  }

  private async runSubscriber(redisUrl: string) {
    const sub = new Redis(redisUrl, { enableOfflineQueue: false });
    sub.on("error", () => {});
    let lastId = "0"; // start from the beginning so early replies are not missed
    for (;;) {
      try {
        const res: unknown = await sub.xread(
          "COUNT",
          16,
          "BLOCK",
          3000,
          "STREAMS",
          RESULTS_STREAM,
          lastId,
        );
        if (res === null) continue;
        // Shape: [[stream, [[id, fields], ...]], ...]
        const batches = res as Array<[string, Array<[string, unknown]>]>;
        for (const batch of batches) {
          const entries = batch[1];
          if (entries === undefined) continue;
          for (const entry of entries) {
            const id = entry[0];
            const raw = readField(entry[1]);
            if (raw !== undefined) {
              try {
                const outcome = JSON.parse(raw) as Result;
                this.deliver(outcome);
              } catch {
                // Unparseable result: ignore, it correlates to nothing.
              }
            }
            lastId = id;
          }
        }
      } catch {
        await new Promise((r) => setTimeout(r, 100));
      }
    }
  }

  private deliver(outcome: Result) {
    const entry = this.pending.get(outcome.reply_key);
    if (!entry) return; // unknown/stale reply_key — dead oneshot
    clearTimeout(entry.timer);
    this.pending.delete(outcome.reply_key);
    entry.resolve(outcome);
  }
}

// ioredis returns stream fields either as an object or a flat key/value array,
// depending on the command/reply decoding. Read `payload` from either shape.
function readField(fields: unknown): string | undefined {
  if (fields === null || typeof fields !== "object") return undefined;
  if (Array.isArray(fields)) {
    const index = fields.indexOf(PAYLOAD);
    return index >= 0 ? (fields[index + 1] as string | undefined) : undefined;
  }
  return (fields as Record<string, string>)[PAYLOAD];
}
