import { useEffect, useRef, useState } from "react";
import { ApiError } from "@/api/problem";
import { DialogShell } from "@/components/DialogShell";
import { Field } from "@/components/Field";
import { FormError } from "@/components/FormError";
import { buildAssetPatch, type Asset } from "@/api/assets";
import { useUpdateAsset } from "./queries";
import { assetClassLabels } from "./labels";

type Props = {
  /** null のときは呼び出し側でこのコンポーネント自体を描画しない */
  asset: Asset;
  onClose: () => void;
};

export function EditAssetDialog({ asset, onClose }: Props) {
  const ref = useRef<HTMLDialogElement>(null);
  const [values, setValues] = useState({
    symbol: asset.symbol,
    name: asset.name,
    price_unit: asset.price_unit,
  });
  const [error, setError] = useState<unknown>(null);
  const [noChange, setNoChange] = useState(false);
  const update = useUpdateAsset(asset.id);

  // key={asset.id} でマウントされた直後に開く。閉じるときは親が破棄する
  useEffect(() => {
    ref.current?.showModal();
  }, []);

  const apiError = error instanceof ApiError ? error : null;
  const fieldErrors = apiError?.fieldErrors ?? {};

  const symbolError =
    apiError?.status === 409
      ? (apiError.problem.detail ?? "このコードは他の銘柄で使われています")
      : fieldErrors.symbol;

  const hasFieldError =
    apiError?.status === 409 || Object.keys(fieldErrors).length > 0;

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    setNoChange(false);

    const patch = buildAssetPatch(asset, values);
    if (patch === null) {
      // 空パッチはサーバーが 400 を返す。手前で止める
      setNoChange(true);
      return;
    }

    try {
      await update.mutateAsync(patch);
      onClose();
    } catch (e) {
      setError(e);
    }
  }

  return (
    <DialogShell dialogRef={ref} onClose={onClose}>
      <form onSubmit={handleSubmit} noValidate className="space-y-4 p-6">
        <h2 className="text-lg font-semibold text-slate-900">銘柄を編集</h2>

        {!hasFieldError && <FormError error={error} />}
        {noChange && (
          <p className="text-sm text-amber-700">変更された項目がありません。</p>
        )}

        <Field
          label="コード"
          name="asset-symbol"
          value={values.symbol}
          onChange={(symbol) => setValues({ ...values, symbol })}
          error={symbolError}
        />

        <Field
          label="名称"
          name="asset-name"
          value={values.name}
          onChange={(name) => setValues({ ...values, name })}
          error={fieldErrors.name}
        />
   
        <Field
          label="価格単位"
          name="asset-price-unit"
          value={values.price_unit}
          onChange={(price_unit) => setValues({ ...values, price_unit })}
          error={fieldErrors.price_unit}
          hint="価格が何口あたりの値かを表します。株式は1、投資信託は10000。空欄なら自動で入ります。"
        />

        <dl className="rounded-md bg-slate-50 px-3 py-2 text-sm text-slate-600">
          <div className="flex justify-between">
            <dt>資産クラス</dt>
            <dd>{assetClassLabels[asset.asset_class]}</dd>
          </div>
          <div className="mt-1 flex justify-between">
            <dt>通貨</dt>
            <dd>{asset.currency}</dd>
          </div>
          <p className="mt-2 text-slate-500">この2項目は変更できません。</p>
        </dl>

        <div className="flex justify-end gap-2 pt-2">
          <button
            type="button"
            onClick={onClose}
            disabled={update.isPending}
            className="rounded-md border border-slate-300 px-4 py-2 text-sm text-slate-700 disabled:opacity-50"
          >
            キャンセル
          </button>
          <button
            type="submit"
            disabled={update.isPending}
            className="rounded-md bg-slate-900 px-4 py-2 text-sm text-white disabled:opacity-50"
          >
            {update.isPending ? "保存中…" : "保存"}
          </button>
        </div>
      </form>
    </DialogShell>
  );
}