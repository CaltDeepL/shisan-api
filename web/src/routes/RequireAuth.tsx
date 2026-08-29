import { Navigate, Outlet, useLocation } from "react-router";
import { useIsAuthenticated } from "@/stores/auth";

export function RequireAuth() {
  const isAuthenticated = useIsAuthenticated();
  const location = useLocation();

  if (!isAuthenticated) {
    // 元いた場所を渡す。replace で履歴を残さない（戻るボタンで往復しないため）
    return <Navigate to="/login" state={{ from: location }} replace />;
  }

  return <Outlet />;
}