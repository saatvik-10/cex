const BASE = "http://127.0.0.1:8000";

type AuthTokens = {
  access_token: string;
  refresh_token: string;
  user: { id: string; username: string };
};

async function request<T = unknown>(
  method: string,
  path: string,
  body?: unknown,
  token?: string,
): Promise<{ status: number; json: T }> {
  const headers: Record<string, string> = { "Content-Type": "application/json" };
  if (token) headers.Authorization = `Bearer ${token}`;

  const res = await fetch(`${BASE}${path}`, {
    method,
    headers,
    body: body === undefined ? undefined : JSON.stringify(body),
  });

  let json = null as T;
  try {
    json = (await res.json()) as T;
  } catch {
    // empty body
  }
  return { status: res.status, json };
}

export async function signup(username: string, password: string) {
  return request<AuthTokens>("POST", "/auth/signup", { username, password });
}

export async function signin(username: string, password: string) {
  return request<AuthTokens>("POST", "/auth/signin", { username, password });
}

export async function refresh(refreshToken: string) {
  return request<AuthTokens>("POST", "/auth/refresh", { refresh_token: refreshToken });
}

export async function profile(accessToken: string) {
  return request<{ id: string; username: string }>("GET", "/auth/profile", undefined, accessToken);
}

export async function balance(accessToken: string) {
  return request<{ balances: { asset: string; amount: string }[] }>(
    "GET",
    "/wallet/balance",
    undefined,
    accessToken,
  );
}

export async function onramp(accessToken: string, asset: string, amount: string) {
  return request<{ asset: string; amount: string }>(
    "POST",
    "/wallet/onramp",
    { asset, amount },
    accessToken,
  );
}
