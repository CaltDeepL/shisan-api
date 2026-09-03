import { useEffect, useRef, useState } from "react";
import { ApiError } from "@/api/problem";
import { DialogShell } from "@/components/DialogShell";
import { FormError } from "@/components/FormError";
import type { Account } from "@/api/accounts";
import { useDeleteAccount } from "./queries";
import { accountTypeLabels } from "./labels";

type Props = {
  /** null のとき閉じている */
  account: Account | null;
  onClose: () => void;
};

export function DeleteAccountDialog({ account, onClose }: Props) {
  const ref = useRef<HTMLDialogElement>(null);
  const [error, setError] = useState<unknown>(null);
  const remove = useDeleteAccount();

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    if (account && !el.open) {
      setError(null);
      el.showModal();
    } else if (!account && el.open) {
      el.close();
    }
  }, [account]);

  if (!account) return <dialog ref={ref} />;

  const apiError = error instanceof ApiError ? error : null;

  // 取引が紐づく口座は FK の ON DELETE RESTRICT で 23503 → 422 になる。
  // 汎用メッセージでは何が起きたか伝わらないので、この場合だけ文言を差し替える
  const restricted = apiError?.status === 422;

  async function handleDelete() {
    setError(null);
    if (!account) return;
    try {
      await remove.mutateAsync(account.id);
      onClose();
    } catch (e) {
      setError(e);
    }
  }

  return (
    <DialogShell dialogRef={ref} onClose={onClose}>
      <div className="space-y-4 p-6">
        <h2 className="text-lg font-semibold text-slate-900">口座を削除</h2>

        <div className="rounded-md border border-slate-200 bg-slate-50 px-3 py-2 text-sm">
          <p className="font-medium text-slate-900">{account.name}</p>
          <p className="text-slate-500">
            {accountTypeLabels[account.account_type]} / {account.currency}
          </p>
        </div>

        <p className="text-sm text-slate-700">
          この口座を削除します。取り消しはできません。
        </p>

        {restricted ? (
          <div
            role="alert"
            className="rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700"
          >
            <p>取引が登録されているため削除できません。</p>
            <p className="mt-1 text-red-600">
              先にこの口座の取引をすべて削除してください。
            </p>
          </div>
        ) : (
          <FormError error={error} />
        )}

        <div className="flex justify-end gap-2 pt-2">
          <button
            type="button"
            onClick={onClose}
            disabled={remove.isPending}
            className="rounded-md border border-slate-300 px-4 py-2 text-sm text-slate-700 disabled:opacity-50"
          >
            キャンセル
          </button>
          <button
            type="button"
            onClick={() => void handleDelete()}
            disabled={remove.isPending || restricted}
            className="rounded-md bg-red-600 px-4 py-2 text-sm text-white hover:bg-red-700 disabled:opacity-50"
          >
            {remove.isPending ? "削除中…" : "削除する"}
          </button>
        </div>
      </div>
    </DialogShell>
  );
}
