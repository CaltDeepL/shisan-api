import { create } from "zustand";
import { persist } from "zustand/middleware";
import { AUTH_EXPIRED, setTokenProvider } from "@/api/client";
import type { TokenResponse } from "@/api/auth";

/** 期限ぎりぎりのトークンを送らないための余裕（ミリ秒） */
const SKEW_MS = 30_000;

type AuthState = {
  token: string | null;
  /** 絶対時刻（epoch ms）。expires_in を受信時刻から換算して保存する */
  expiresAt: number | null;
  setSession: (res: TokenResponse) => void;
  logout: () => void;
};

export const useAuthStore = create<AuthState>()(
  persist(
    (set) => ({
      token: null,
      expiresAt: null,
      setSession: (res) =>
        set({
          token: res.access_token,
          expiresAt: Date.now() + res.expires_in * 1000,
        }),
      logout: () => set({ token: null, expiresAt: null }),
    }),
    { name: "asset-log-auth" },
  ),
);

/** 有効期限内のトークンだけを返す。期限切れは null 扱い。 */
export function currentToken(): string | null {
  const { token, expiresAt } = useAuthStore.getState();
  if (!token || !expiresAt) return null;
  if (Date.now() >= expiresAt - SKEW_MS) return null;
  return token;
}

/** ログイン済みかどうか。コンポーネントからはこれを購読する。 */
export function useIsAuthenticated(): boolean {
  return useAuthStore((s) => {
    if (!s.token || !s.expiresAt) return false;
    return Date.now() < s.expiresAt - SKEW_MS;
  });
}

/** アプリ起動時に一度だけ呼ぶ。 */
export function initAuth() {
  setTokenProvider(currentToken);
  window.addEventListener(AUTH_EXPIRED, () => {
    useAuthStore.getState().logout();
  });
}
