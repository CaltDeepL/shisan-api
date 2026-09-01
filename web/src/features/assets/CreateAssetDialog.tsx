import { useEffect, useRef, useState } from "react";
import { ApiError } from "@/api/problem";
import { Field } from "@/components/Field";
import { FormError } from "@/components/FormError";
import { buildCreateAsset, type AssetClass } from "@/api/assets";
import { useCreateAsset } from "./queries";
import { assetClassLabels, assetClassOptions, currencyOptions } from "./labels";

type Props = {
  open: boolean;
  onClose: () => void;
};

const initialValues = {
  symbol: "",
  name: "",
  asset_class: "equity" as AssetClass,
  currency: "JPY",
  price_unit: "",
};

export function CreateAssetDialog({ open, onClose }: Props) {
  const ref = useRef<HTMLDialogElement>(null);
  const [values, setValues] = useState(initialValues);
  const [error, setError] = useState<unknown>(null);
  const create = useCreateAsset();

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    if (open && !el.open) {
      setValues(initialValues);
      setError(null);
      el.showModal();
    } else if (!open && el.open) {
      el.close();
    }
  }, [open]);

  const apiError = error instanceof ApiError ? error : null;
  const fieldErrors = apiError?.fieldErrors ?? {};

  // 409 は errors[] を持たないので、シンボルの重複として自前で紐づける
  const symbolError =
    apiError?.status === 409
      ? (apiError.problem.detail ?? "このコードは既に登録されています")
      : fieldErrors.symbol;

  const hasFieldError =
    apiError?.status === 409 || Object.keys(fieldErrors).length > 0;

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    try {
      await create.mutateAsync(buildCreateAsset(values));
      onClose();
    } catch (e) {
      setError(e);
    }
  }

  return (
    <dialog
      ref={ref}
      onCancel={onClose}
      onClose={onClose}
      className="w-full max-w-md rounded-lg p-0 backdrop:bg-slate-900/40"
    >
      <form onSubmit={handleSubmit} noValidate className="space-y-4 p-6">
        <h2 className="text-lg font-semibold text-slate-900">銘柄を追加</h2>

        {!hasFieldError && <FormError error={error} />}

        <Field
          label="コード"
          name="asset-symbol"
          value={values.symbol}
          onChange={(symbol) => setValues({ ...values, symbol })}
          error={symbolError}
          hint="証券コードやティッカー。例: 7203、VOO"
        />

        <Field
          label="名称"
          name="asset-name"
          value={values.name}
          onChange={(name) => setValues({ ...values, name })}
          error={fieldErrors.name}
        />

        <div className="space-y-1">
          <label
            htmlFor="asset-class"
            className="block text-sm font-medium text-slate-700"
          >
            資産クラス
          </label>
          <select
            id="asset-class"
            name="asset-class"
            value={values.asset_class}
            onChange={(e) =>
              setValues({
                ...values,
                asset_class: e.target.value as AssetClass,
              })
            }
            className="w-full rounded-md border border-slate-300 px-3 py-2 text-slate-900 outline-none focus:ring-2 focus:ring-slate-300"
          >
            {assetClassOptions.map((c) => (
              <option key={c} value={c}>
                {assetClassLabels[c]}
              </option>
            ))}
          </select>
          <p className="text-sm text-slate-500">作成後は変更できません。</p>
        </div>

        <div className="space-y-1">
          <label
            htmlFor="asset-currency"
            className="block text-sm font-medium text-slate-700"
          >
            通貨
          </label>
          <select
            id="asset-currency"
            name="asset-currency"
            value={values.currency}
            onChange={(e) =>
              setValues({ ...values, currency: e.target.value })
            }
            className="w-full rounded-md border border-slate-300 px-3 py-2 text-slate-900 outline-none focus:ring-2 focus:ring-slate-300"
          >
            {currencyOptions.map((c) => (
              <option key={c} value={c}>
                {c}
              </option>
            ))}
          </select>
          {fieldErrors.currency && (
            <p className="text-sm text-red-600">{fieldErrors.currency}</p>
          )}
          <p className="text-sm text-slate-500">作成後は変更できません。</p>
        </div>

        <Field
          label="価格単位"
          name="asset-price-unit"
          value={values.price_unit}
          onChange={(price_unit) => setValues({ ...values, price_unit })}
          error={fieldErrors.price_unit}
          hint="価格が何口あたりの値かを表します。株式は1、投資信託は10000。空欄なら自動で入ります。"
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
            {create.isPending ? "作成中…" : "作成"}
          </button>
        </div>
      </form>
    </dialog>
  );
}