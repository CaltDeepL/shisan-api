import { Link, Outlet } from "react-router";
import { useAuthStore } from "@/stores/auth";

export function AppLayout() {
  const logout = useAuthStore((s) => s.logout);

  return (
    <div className="min-h-screen bg-slate-50">
      <header className="flex items-center justify-between border-b bg-white px-6 py-3">
        <Link to="/" className="font-semibold text-slate-800">
          asset-log
        </Link>
        <button
          onClick={logout}
          className="text-sm text-slate-500 hover:text-slate-900"
        >
          ログアウト
        </button>
      </header>
      <main className="mx-auto max-w-5xl p-6">
        <Outlet />
      </main>
    </div>
  );
}