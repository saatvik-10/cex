import { describe, expect, test } from "bun:test";
import { balance, deposit, profile, signup } from "./client";

const PASSWORD = "integration-123";

function uniqueUser(prefix: string) {
  return `${prefix}-${Date.now()}-${Math.floor(Math.random() * 1e6)}`;
}

async function usdOf(token: string) {
  const { json: b } = await balance(token);
  return b.balances.find((e) => e.asset === "USD")?.amount ?? "0";
}

describe("demo money", () => {
  test("signup seeds the demo basket and marks the account demo", async () => {
    const username = uniqueUser("demo");
    const { status, json } = await signup(username, PASSWORD);
    expect(status).toBe(200);
    expect(json.user.isDemo).toBe(true);

    const { status: pStatus, json: p } = await profile(json.access_token);
    expect(pStatus).toBe(200);
    expect(p.isDemo).toBe(true);

    expect(await usdOf(json.access_token)).toBe("10000.000000000000000000");
  });

  test("faucet tops up the basket and stops at the cap", async () => {
    const { json } = await signup(uniqueUser("faucet"), PASSWORD);
    const token = json.access_token;

    // Seed 26_500 USD-equiv (10k + 10*150 + 5*3000); two taps fit under the
    // 100k cap, a third would overshoot.
    expect(await usdOf(token)).toBe("10000.000000000000000000");

    const first = await fetch("http://127.0.0.1:8000/demo/funds", {
      method: "POST",
      headers: { Authorization: `Bearer ${token}` },
    });
    expect(first.status).toBe(200);
    expect(await usdOf(token)).toBe("20000.000000000000000000");

    const second = await fetch("http://127.0.0.1:8000/demo/funds", {
      method: "POST",
      headers: { Authorization: `Bearer ${token}` },
    });
    expect(second.status).toBe(200);
    expect(await usdOf(token)).toBe("30000.000000000000000000");

    // Current = 79,500 USD-equiv; one more basket (26,500) -> 106,000 > cap.
    const third = await fetch("http://127.0.0.1:8000/demo/funds", {
      method: "POST",
      headers: { Authorization: `Bearer ${token}` },
    });
    expect(third.status).toBe(400);
    expect(((await third.json()) as { error: string }).error).toBe(
      "demo balance cap reached",
    );
  });

  test("faucet requires auth; deposit is capped by the total", async () => {
    const { json } = await signup(uniqueUser("cap"), PASSWORD);
    const token = json.access_token;

    const anon = await fetch("http://127.0.0.1:8000/demo/funds", {
      method: "POST",
    });
    expect(anon.status).toBe(401);

    // Seed = 26,500 USD-equiv. 26,500 + 20,000 + 53,500 = 100,000 = cap
    // exactly (allowed); the next deposit pushes past it.
    const ok = await deposit(token, "USD", "20000");
    expect(ok.status).toBe(200);
    const boundary = await deposit(token, "USD", "53500");
    expect(boundary.status).toBe(200);
    expect(await usdOf(token)).toBe("83500.000000000000000000");

    const over = await deposit(token, "USD", "1");
    expect(over.status).toBe(400);
    expect((over.json as { error: string }).error).toBe("demo balance cap exceeded");
    expect(await usdOf(token)).toBe("83500.000000000000000000");
  });
});