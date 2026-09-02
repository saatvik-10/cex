import { describe, expect, test } from "bun:test";
import { balance, onramp, signup } from "./client";

const PASSWORD = "integration-123";

function uniqueUser(prefix: string) {
  return `${prefix}-${Date.now()}-${Math.floor(Math.random() * 1e6)}`;
}

describe("wallet flow", () => {
  test("new user starts with zero balances across all assets", async () => {
    const username = uniqueUser("wallet");
    const { json } = await signup(username, PASSWORD);

    const { status, json: b } = await balance(json.access_token);
    expect(status).toBe(200);

    const byAsset = Object.fromEntries(b.balances.map((e) => [e.asset, e.amount]));
    expect(byAsset).toEqual({ USD: "0", SOL: "0", ETH: "0" });
  });

  test("onramp credits the balance and is reflected on read", async () => {
    const username = uniqueUser("onramp");
    const { json } = await signup(username, PASSWORD);
    const token = json.access_token;

    const credit = await onramp(token, "USD", "100");
    expect(credit.status).toBe(200);
    expect(credit.json.asset).toBe("USD");

    const { json: b } = await balance(token);
    const usd = b.balances.find((e) => e.asset === "USD");
    expect(usd?.amount).toBe("100.000000000000000000");
  });

  test("onramp accumulates across multiple credits", async () => {
    const username = uniqueUser("accum");
    const { json } = await signup(username, PASSWORD);
    const token = json.access_token;

    await onramp(token, "SOL", "1.5");
    const second = await onramp(token, "SOL", "0.5");

    expect(second.status).toBe(200);
    expect(second.json.amount).toBe("2.000000000000000000");

    const { json: b } = await balance(token);
    const sol = b.balances.find((e) => e.asset === "SOL");
    expect(sol?.amount).toBe("2.000000000000000000");
  });

  test("onramp rejects a non-positive amount", async () => {
    const username = uniqueUser("reject");
    const { json } = await signup(username, PASSWORD);
    const token = json.access_token;

    const { status } = await onramp(token, "USD", "-5");
    expect(status).toBe(400);
  });

  test("onramp without a token is unauthorized", async () => {
    const { status } = await onramp("bad-token", "USD", "10");
    expect(status).toBe(401);
  });
});
