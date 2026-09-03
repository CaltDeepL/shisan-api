import { useEffect, useRef, useState } from "react";
import { ApiError } from "@/api/problem";
import { DialogShell } from "@/components/DialogShell";
import { FormError } from "@/components/FormError";
import type { Transaction } from "@/api/transactions";
import { useDeleteTransaction } from "./queries";

type Props = {
  transaction: Transaction;
  /** 確認文に出す取引の要約 */
  label: string;
  onClose: () => void;
};

export function DeleteTransactionDialog({ transaction, label, onClose }: Props) {
  const ref = useRef<HTMLDialogElement>(null);
  const [error, setError] = useState<unknown>(null);
  const remove = useDeleteTransaction();

  useEffect(() => {
    ref.current?.showModal();
  }, []);

  const apiError = error instanceof ApiError ? error : null;
  // 422（後続の売却が保有数量を超える）は再試行しても結果が変わらない
  const blocked = apiError?.status === 422;

  async function handleDelete() {
    setError(null);
    try {
      await remove.mutateAsync(transaction.id);
      onClose();
    } catch (e) {
      setError(e);
    }
  }

  return (
    <DialogShell dialogRef={ref} onClose={onClose}>
      <div className="space-y-4 p-6">
        <h2 className="text-lg font-semibold text-slate-900">取引を削除</h2>
        <p className="text-sm text-slate-700">{label}</p>
        <p className="text-sm text-slate-500">
          この操作は取り消せません。訂正したい場合は、削除してから登録し直してください。
        </p>

        {/* サーバーの detail が十分に具体的なので書き換えない */}
        <FormError error={error} />

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
            disabled={remove.isPending || blocked}
            className="rounded-md bg-red-600 px-4 py-2 text-sm text-white disabled:opacity-50"
          >
            {remove.isPending ? "削除中…" : "削除"}
          </button>
        </div>
      </div>
    </DialogShell>
  );
}