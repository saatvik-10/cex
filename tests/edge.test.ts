import { describe, expect, test } from "bun:test";
import { balance, deposit, signup } from "./client";

describe("edge cases", () => {
  test("amount accepted as JSON number", async () => {
    const { status, json } = await signup("edge_num", "password123");
    expect(status).toBe(200);
    const r = await deposit(
      json.access_token,
      "USD",
      "100" as unknown as string,
    );
    // deposit() stringifies; issue the raw number body instead:
    const res = await fetch("http://127.0.0.1:8000/deposit/USD", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Authorization: `Bearer ${json.access_token}`,
      },
      body: JSON.stringify({ amount: 50 }),
    });
    expect(res.status).toBe(200);
    const body = (await res.json()) as { asset: string; amount: string };
    expect(body.asset).toBe("USD");
    expect(body.amount).toBe("10150.000000000000000000");
  });

  test("decimal spellings .5 and 5. are accepted", async () => {
    const { status, json } = await signup("edge_dec", "password123");
    expect(status).toBe(200);
    const dot = await fetch("http://127.0.0.1:8000/deposit/USD", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Authorization: `Bearer ${json.access_token}`,
      },
      body: JSON.stringify({ amount: ".5" }),
    });
    expect(dot.status).toBe(200);
    const suffix = await fetch("http://127.0.0.1:8000/deposit/USD", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Authorization: `Bearer ${json.access_token}`,
      },
      body: JSON.stringify({ amount: "5." }),
    });
    expect(suffix.status).toBe(200);
    const b = await balance(json.access_token);
    expect(b.status).toBe(200);
    expect(b.json.balances.find((x) => x.asset === "USD")?.amount).toBe(
      "10005.500000000000000000",
    );
  });

  test("money rejects zero, negatives, excess precision, malformed", async () => {
    const { json } = await signup("edge_bad", "password123");
    const badBodies = [
      { amount: 0 },
      { amount: -5 },
      { amount: "-1" },
      { amount: "0.0000000000000000001" },
      { amount: "1.1234567890123456789" },
      { amount: "abc" },
    ];
    for (const body of badBodies) {
      const res = await fetch("http://127.0.0.1:8000/deposit/USD", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Authorization: `Bearer ${json.access_token}`,
        },
        body: JSON.stringify(body),
      });
      expect(res.status, JSON.stringify(body)).toBe(400);
    }
    const b = await balance(json.access_token);
    const usd = b.json.balances.find((x) => x.asset === "USD")?.amount;
    expect(usd).toBe("10000.000000000000000000");
  });

  test("asset matching and positivity are case-insensitive for asset", async () => {
    const { json } = await signup("edge_lower", "password123");
    const low = await fetch("http://127.0.0.1:8000/deposit/sol", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Authorization: `Bearer ${json.access_token}`,
      },
      body: JSON.stringify({ amount: "2" }),
    });
    expect(low.status).toBe(200);
    const b = await balance(json.access_token);
    expect(b.json.balances.find((x) => x.asset === "SOL")?.amount).toBe(
      "12.000000000000000000",
    );
  });

  test("health and JSON 404 envelope", async () => {
    const health = await fetch("http://127.0.0.1:8000/health");
    expect(health.status).toBe(200);
    expect(await health.json()).toEqual({ status: "ok" });
    const missing = await fetch("http://127.0.0.1:8000/nope");
    expect(missing.status).toBe(404);
    expect(await missing.json()).toEqual({ error: "not found" });
  });
});
