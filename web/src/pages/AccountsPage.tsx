import { useState } from "react";
import { useAccounts } from "@/features/accounts/queries";
import {
  accountTypeLabels,
  withholdingLabel,
} from "@/features/accounts/labels";
import { CreateAccountDialog } from "@/features/accounts/CreateAccountDialog";
import { EditAccountDialog } from "@/features/accounts/EditAccountDialog";
import type { Account } from "@/api/accounts";
import { DeleteAccountDialog } from "@/features/accounts/DeleteAccountDialog";

export function AccountsPage() {
  const [creating, setCreating] = useState(false);
  const [editing, setEditing] = useState<Account | null>(null);
  const [deleting, setDeleting] = useState<Account | null>(null);
  const { data, isPending, isError, error, refetch } = useAccounts();
  const addButton = (
    <button
      type="button"
      onClick={() => setCreating(true)}
      className="rounded-md bg-slate-900 px-4 py-2 text-sm text-white hover:bg-slate-700"
    >
      口座を追加
    </button>
  );

  if (isPending) {
    return <p className="text-sm text-slate-500">読み込み中…</p>;
  }

  if (isError) {
    return (
      <div
        role="alert"
        className="rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700"
      >
        <p>口座の取得に失敗しました。</p>
        <p className="mt-1 text-red-500">{error.message}</p>
        <button
          type="button"
          onClick={() => void refetch()}
          className="mt-2 rounded-md border border-red-300 px-3 py-1 text-red-700"
        >
          再試行
        </button>
      </div>
    );
  }

  return (
    <section className="space-y-6">
      <header className="flex items-center justify-between">
        <h1 className="text-xl font-semibold text-slate-900">口座</h1>
        {data.length > 0 && addButton}
      </header>

      {data.length === 0 ? (
        <div className="rounded-lg border border-dashed border-slate-300 px-6 py-12 text-center">
          <p className="text-slate-900">まだ口座が登録されていません。</p>
          <p className="mt-1 text-sm text-slate-500">
            証券口座や銀行口座を登録すると、取引や資産の記録ができます。
          </p>
          <div className="mt-4">{addButton}</div>
        </div>
      ) : (
        <div className="overflow-hidden rounded-lg border border-slate-200">
          <table className="w-full text-sm">
            <thead className="bg-slate-50 text-left text-slate-600">
              <tr>
                <th scope="col" className="px-4 py-2 font-medium">
                  口座名
                </th>
                <th scope="col" className="px-4 py-2 font-medium">
                  種別
                </th>
                <th scope="col" className="px-4 py-2 font-medium">
                  源泉徴収
                </th>
                <th scope="col" className="px-4 py-2 font-medium">
                  金融機関
                </th>
                <th scope="col" className="px-4 py-2 font-medium">
                  通貨
                </th>
                <th scope="col" className="px-4 py-2">
                  <span className="sr-only">操作</span>
                </th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-200 text-slate-900">
              {data.map((a) => (
                <tr key={a.id}>
                  <td className="px-4 py-2">{a.name}</td>
                  <td className="px-4 py-2">
                    {accountTypeLabels[a.account_type]}
                  </td>
                  <td className="px-4 py-2">
                    {withholdingLabel(a.withholding ?? null)}
                  </td>
                  <td className="px-4 py-2">{a.institution ?? "—"}</td>
                  <td className="px-4 py-2">{a.currency}</td>
                  <td className="px-4 py-2 text-right">
                    <button
                      type="button"
                      onClick={() => setEditing(a)}
                      className="rounded-md border border-slate-300 px-3 py-1 text-slate-700"
                    >
                      編集
                    </button>
                    <button
                      type="button"
                      onClick={() => setDeleting(a)}
                      className="ml-2 rounded-md border border-slate-300 px-3 py-1 text-red-600"
                    >
                      削除
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      <CreateAccountDialog open={creating} onClose={() => setCreating(false)} />
      <EditAccountDialog account={editing} onClose={() => setEditing(null)} />
      <DeleteAccountDialog
        account={deleting}
        onClose={() => setDeleting(null)}
      />
    </section>
  );
}
