import { describe, expect, test } from "bun:test";
import { balance, deposit, signup } from "./client";

const PASSWORD = "integration-123";

function uniqueUser(prefix: string) {
  return `${prefix}-${Date.now()}-${Math.floor(Math.random() * 1e6)}`;
}

describe("wallet flow", () => {
  test("new user starts with the demo basket across all assets", async () => {
    const username = uniqueUser("wallet");
    const { json } = await signup(username, PASSWORD);

    const { status, json: b } = await balance(json.access_token);
    expect(status).toBe(200);

    const byAsset = Object.fromEntries(
      b.balances.map((e) => [e.asset, e.amount]),
    );
    expect(byAsset).toEqual({
      USD: "10000.000000000000000000",
      SOL: "10.000000000000000000",
      ETH: "5.000000000000000000",
    });
  });

  test("deposit credits the balance and is reflected on read", async () => {
    const username = uniqueUser("deposit");
    const { json } = await signup(username, PASSWORD);
    const token = json.access_token;

    const credit = await deposit(token, "USD", "100");
    expect(credit.status).toBe(200);
    expect(credit.json.asset).toBe("USD");

    const { json: b } = await balance(token);
    const usd = b.balances.find((e) => e.asset === "USD");
    expect(usd?.amount).toBe("10100.000000000000000000");
  });

  test("deposit accumulates across multiple credits", async () => {
    const username = uniqueUser("accum");
    const { json } = await signup(username, PASSWORD);
    const token = json.access_token;

    await deposit(token, "SOL", "1.5");
    const second = await deposit(token, "SOL", "0.5");

    expect(second.status).toBe(200);
    expect(second.json.amount).toBe("12.000000000000000000");

    const { json: b } = await balance(token);
    const sol = b.balances.find((e) => e.asset === "SOL");
    expect(sol?.amount).toBe("12.000000000000000000");
  });

  test("deposit rejects a non-positive amount", async () => {
    const username = uniqueUser("reject");
    const { json } = await signup(username, PASSWORD);
    const token = json.access_token;

    const { status } = await deposit(token, "USD", "-5");
    expect(status).toBe(400);
  });

  test("deposit without a token is unauthorized", async () => {
    const { status } = await deposit("bad-token", "USD", "10");
    expect(status).toBe(401);
  });
});
