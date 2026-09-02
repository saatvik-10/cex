import { describe, expect, test } from "bun:test";
import { profile, refresh, signin, signup } from "./client";

const PASSWORD = "integration-123";

function uniqueUser(prefix: string) {
  return `${prefix}-${Date.now()}-${Math.floor(Math.random() * 1e6)}`;
}

describe("auth flow", () => {
  test("signup returns access + refresh tokens and a user", async () => {
    const username = uniqueUser("alice");
    const { status, json } = await signup(username, PASSWORD);

    expect(status).toBe(200);
    expect(json.access_token).toBeString();
    expect(json.refresh_token).toBeString();
    expect(json.user.username).toBe(username);
    expect(json.user.id).toBeString();
  });

  test("duplicate signup is rejected with a conflict", async () => {
    const username = uniqueUser("dup");
    await signup(username, PASSWORD);

    const { status, json } = await signup(username, PASSWORD);
    expect(status).toBe(409);
  });

  test("signin with correct password returns tokens", async () => {
    const username = uniqueUser("bob");
    await signup(username, PASSWORD);

    const { status, json } = await signin(username, PASSWORD);
    expect(status).toBe(200);
    expect(json.access_token).toBeString();
    expect(json.refresh_token).toBeString();
  });

  test("signin with wrong password is unauthorized", async () => {
    const username = uniqueUser("carol");
    await signup(username, PASSWORD);

    const { status } = await signin(username, "wrong-password");
    expect(status).toBe(401);
  });

  test("profile returns the current user for a valid token", async () => {
    const username = uniqueUser("dave");
    const { json } = await signup(username, PASSWORD);

    const { status, json: profileJson } = await profile(json.access_token);
    expect(status).toBe(200);
    expect(profileJson.username).toBe(username);
  });

  test("profile without a token is unauthorized", async () => {
    const { status } = await profile("not-a-token");
    expect(status).toBe(401);
  });

  test("refresh rotates the refresh token and returns a fresh pair", async () => {
    const username = uniqueUser("erin");
    const { json } = await signup(username, PASSWORD);

    const { status, json: refreshed } = await refresh(json.refresh_token);
    expect(status).toBe(200);
    expect(refreshed.access_token).toBeString();
    expect(refreshed.refresh_token).toBeString();
    // Rotation invalidates the old refresh token.
    const { status: reused } = await refresh(json.refresh_token);
    expect(reused).toBe(401);
  });
});
