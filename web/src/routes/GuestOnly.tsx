import { Navigate, Outlet } from "react-router";
import { useIsAuthenticated } from "@/stores/auth";

/** ログイン/登録ページ用。認証済みならダッシュボードへ流す。 */
export function GuestOnly() {
  const isAuthenticated = useIsAuthenticated();

  if (isAuthenticated) {
    return <Navigate to="/" replace />;
  }

  return <Outlet />;
}
