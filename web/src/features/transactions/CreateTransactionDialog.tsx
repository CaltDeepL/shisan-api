import { useEffect, useRef, useState } from "react";
import { ApiError } from "@/api/problem";
import { DialogShell } from "@/components/DialogShell";
import { Field } from "@/components/Field";
import { FormError } from "@/components/FormError";
import type { Account } from "@/api/accounts";
import type { Asset } from "@/api/assets";
import {
  buildCreateTransaction,
  type TradeKind,
  type TransactionFormValues,
} from "@/api/transactions";
import { useCreateTransaction } from "./queries";
import { tradeKindLabels } from "./labels";

type Props = {
  open: boolean;
  onClose: () => void;
  accounts: Account[];
  assets: Asset[];
};

function today() {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

export function CreateTransactionDialog({
  open,
  onClose,
  accounts,
  assets,
}: Props) {
  const ref = useRef<HTMLDialogElement>(null);
  const initial: TransactionFormValues = {
    account_id: accounts[0]?.id ?? "",
    asset_id: assets[0]?.id ?? "",
    kind: "buy",
    quantity: "",
    price: "",
    fee: "",
    traded_at: today(),
    note: "",
  };
  const [values, setValues] = useState(initial);
  const [error, setError] = useState<unknown>(null);
  const create = useCreateTransaction();

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    if (open && !el.open) {
      setValues(initial);
      setError(null);
      el.showModal();
    } else if (!open && el.open) {
      el.close();
    }
    // initial は open のたびに作り直される。依存に入れると無限ループになる
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  const apiError = error instanceof ApiError ? error : null;
  const fieldErrors = apiError?.fieldErrors ?? {};
  const hasFieldError = Object.keys(fieldErrors).length > 0;

  const selectedAsset = assets.find((a) => a.id === values.asset_id);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    try {
      await create.mutateAsync(buildCreateTransaction(values));
      onClose();
    } catch (e) {
      setError(e);
    }
  }

  return (
    <DialogShell dialogRef={ref} onClose={onClose}>
      <form onSubmit={handleSubmit} noValidate className="space-y-4 p-6">
        <h2 className="text-lg font-semibold text-slate-900">取引を追加</h2>

        {/* 売却超過は errors[] を持たない unprocessable なので、
            fieldErrors が空でも detail をここに出す必要がある */}
        {!hasFieldError && <FormError error={error} />}

        <div className="space-y-1">
          <label htmlFor="tx-account" className="block text-sm font-medium text-slate-700">
            口座
          </label>
          <select
            id="tx-account"
            value={values.account_id}
            onChange={(e) => setValues({ ...values, account_id: e.target.value })}
            className="w-full rounded-md border border-slate-300 px-3 py-2 text-slate-900"
          >
            {accounts.map((a) => (
              <option key={a.id} value={a.id}>{a.name}</option>
            ))}
          </select>
        </div>

        <div className="space-y-1">
          <label htmlFor="tx-asset" className="block text-sm font-medium text-slate-700">
            銘柄
          </label>
          <select
            id="tx-asset"
            value={values.asset_id}
            onChange={(e) => setValues({ ...values, asset_id: e.target.value })}
            className="w-full rounded-md border border-slate-300 px-3 py-2 text-slate-900"
          >
            {assets.map((a) => (
              <option key={a.id} value={a.id}>{a.symbol} {a.name}</option>
            ))}
          </select>
        </div>

        <fieldset className="space-y-1">
          <legend className="block text-sm font-medium text-slate-700">種別</legend>
          <div className="flex gap-4 pt-1">
            {(["buy", "sell"] as const).map((k) => (
              <label key={k} className="flex items-center gap-2 text-sm text-slate-900">
                <input
                  type="radio"
                  name="kind"
                  checked={values.kind === k}
                  onChange={() => setValues({ ...values, kind: k as TradeKind })}
                />
                {tradeKindLabels[k]}
              </label>
            ))}
          </div>
        </fieldset>

        {/* Decimal を崩さないため type="number" は使わない */}
        <Field
          label="数量"
          name="quantity"
          value={values.quantity}
          onChange={(quantity) => setValues({ ...values, quantity })}
          error={fieldErrors.quantity}
          inputMode="decimal"
        />

        <Field
          label="単価"
          name="price"
          value={values.price}
          onChange={(price) => setValues({ ...values, price })}
          error={fieldErrors.price}
          inputMode="decimal"
          hint={
            selectedAsset
              ? `${selectedAsset.currency} / ${selectedAsset.price_unit}口あたり`
              : undefined
          }
        />

        <Field
          label="手数料"
          name="fee"
          value={values.fee}
          onChange={(fee) => setValues({ ...values, fee })}
          error={fieldErrors.fee}
          inputMode="decimal"
          hint="任意。空欄なら0になります。"
        />

        <div className="space-y-1">
          <label htmlFor="tx-traded-at" className="block text-sm font-medium text-slate-700">
            約定日
          </label>
          <input
            id="tx-traded-at"
            type="date"
            value={values.traded_at}
            max={today()}
            onChange={(e) => setValues({ ...values, traded_at: e.target.value })}
            className="w-full rounded-md border border-slate-300 px-3 py-2 text-slate-900"
          />
          {fieldErrors.traded_at && (
            <p className="text-sm text-red-600">{fieldErrors.traded_at}</p>
          )}
        </div>

        <Field
          label="メモ"
          name="note"
          value={values.note}
          onChange={(note) => setValues({ ...values, note })}
          error={fieldErrors.note}
          hint="任意。"
        />

        <div className="flex justify-end gap-2 pt-2">
          <button
            type="button"
            onClick={onClose}
            disabled={create.isPending}
            className="rounded-md border border-slate-300 px-4 py-2 text-sm text-slate-700 disabled:opacity-50"
          >
            キャンセル
          </button>
          <button
            type="submit"
            disabled={create.isPending}
            className="rounded-md bg-slate-900 px-4 py-2 text-sm text-white disabled:opacity-50"
          >
            {create.isPending ? "登録中…" : "登録"}
          </button>
        </div>
      </form>
    </DialogShell>
  );
}