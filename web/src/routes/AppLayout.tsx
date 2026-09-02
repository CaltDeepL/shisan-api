import { Link, NavLink, Outlet } from "react-router";
import { useAuthStore } from "@/stores/auth";

const navItems = [
  { to: "/", label: "ダッシュボード", end: true },
  { to: "/accounts", label: "口座" },
  { to: "/holdings", label: "保有一覧" },
  { to: "/transactions", label: "取引" },
  { to: "/assets", label: "銘柄" },
  { to: "/analytics", label: "分析" },
  { to: "/import", label: "CSVインポート" },
];

export function AppLayout() {
  const logout = useAuthStore((s) => s.logout);

  return (
    <div className="min-h-screen bg-slate-50">
      <header className="flex items-center justify-between border-b bg-white px-6 py-3">
        <div className="flex items-center gap-6">
          <Link to="/" className="font-semibold text-slate-800">
            asset-log
          </Link>
          <nav className="flex gap-4">
            {navItems.map(({ to, label, end }) => (
              <NavLink
                key={to}
                to={to}
                end={end}
                className={({ isActive }) =>
                  `text-sm ${isActive ? "font-medium text-slate-900" : "text-slate-500 hover:text-slate-900"}`
                }
              >
                {label}
              </NavLink>
            ))}
          </nav>
        </div>
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
