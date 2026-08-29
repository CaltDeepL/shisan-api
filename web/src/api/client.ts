import { ApiError, type ProblemDetails } from "./problem";

const BASE = import.meta.env.VITE_API_BASE_URL;

/** トークンの取得口。store を直接 import すると循環参照になるため注入する。 */
let getToken: () => string | null = () => null;
export function setTokenProvider(fn: () => string | null) {
  getToken = fn;
}

/** 認証付きリクエストが 401 を返したときに発火する。 */
export const AUTH_EXPIRED = "auth:expired";

type RequestOptions = {
  method?: "GET" | "POST" | "PATCH" | "DELETE";
  body?: unknown;
  /** false にすると Authorization を付けず、401 でも自動ログアウトしない */
  auth?: boolean;
};

async function readProblem(res: Response): Promise<ProblemDetails> {
  const ct = res.headers.get("content-type") ?? "";
  if (ct.includes("json")) {
    try {
      return (await res.json()) as ProblemDetails;
    } catch {
      /* 本文が壊れている場合は下へ */
    }
  }
  return { status: res.status, title: `HTTP ${res.status}` };
}

export async function apiFetch<T>(
  path: string,
  { method = "GET", body, auth = true }: RequestOptions = {},
): Promise<T> {
  const token = auth ? getToken() : null;

  const headers: Record<string, string> = {};
  if (body !== undefined) headers["content-type"] = "application/json";
  if (token) headers.authorization = `Bearer ${token}`;

  let res: Response;
  try {
    res = await fetch(`${BASE}${path}`, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
    });
  } catch {
    // CORS 拒否・オフライン・DNS 失敗はすべてここに来る。原因はJSからは判別できない。
    throw new ApiError(0, {
      title: "通信エラー",
      detail: "サーバーに接続できませんでした",
    });
  }

  // トークンを付けたのに 401 = 期限切れか無効。付けていない 401 は認証情報の誤り。
  if (res.status === 401 && token) {
    window.dispatchEvent(new Event(AUTH_EXPIRED));
  }

  if (!res.ok) throw new ApiError(res.status, await readProblem(res));
  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}