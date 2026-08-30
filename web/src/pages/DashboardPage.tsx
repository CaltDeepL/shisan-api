import { useQuery } from "@tanstack/react-query";
import { fetchMe } from "@/api/auth";

export function DashboardPage() {
  const { data, isPending, error } = useQuery({
    queryKey: ["me"],
    queryFn: fetchMe,
  });

  if (isPending) return <p className="text-slate-500">読み込み中…</p>;
  if (error) return <p className="text-red-600">{error.message}</p>;

  return (
    <div className="space-y-2">
      <h1 className="text-xl font-semibold">ログイン済み</h1>
      <p className="text-sm text-slate-500">user_id: {data.user_id}</p>
    </div>
  );
}
