import { useEffect, useRef, useState } from "react";
import { ApiError } from "@/api/problem";
import { Field } from "@/components/Field";
import { FormError } from "@/components/FormError";
import {
  buildAccountPatch,
  type Account,
  type AccountFormValues,
} from "@/api/accounts";
import { useUpdateAccount } from "./queries";
import { accountTypeLabels } from "./labels";

type Props = {
  /** null のとき閉じている。開くたびにこの値でフォームを初期化する */
  account: Account | null;
  onClose: () => void;
};

function toFormValues(a: Account): AccountFormValues {
  return {
    name: a.name,
    institution: a.institution ?? "",
    withholding: a.withholding ?? null,
  };
}

export function EditAccountDialog({ account, onClose }: Props) {
  const ref = useRef<HTMLDialogElement>(null);
  const [values, setValues] = useState<AccountFormValues>({
    name: "",
    institution: "",
    withholding: null,
  });
  const [error, setError] = useState<unknown>(null);
  const [noChange, setNoChange] = useState(false);

  // account が null でない間だけ id が確定する。hooks は常に呼ぶ必要があるので空文字で凌ぐ
  const update = useUpdateAccount(account?.id ?? "");

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    if (account && !el.open) {
      setValues(toFormValues(account));
      setError(null);
      setNoChange(false);
      el.showModal();
    } else if (!account && el.open) {
      el.close();
    }
  }, [account]);

  if (!account) return <dialog ref={ref} />;

  const apiError = error instanceof ApiError ? error : null;
  const fieldErrors = apiError?.fieldErrors ?? {};
  const nameError =
    apiError?.status === 409
      ? "同じ名前の口座が既に登録されています"
      : fieldErrors.name;
  const hasFieldError =
    apiError?.status === 409 || Object.keys(fieldErrors).length > 0;

  const isTokutei = account.account_type === "tokutei";

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    setNoChange(false);

    if (!account) return;

    const patch = buildAccountPatch(account, values);
    if (patch === null) {
      // 空パッチをそのまま送るとサーバーが 400 を返すので、手前で止める
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
    <dialog
      ref={ref}
      onCancel={onClose}
      onClose={onClose}
      className="w-full max-w-md rounded-lg p-0 backdrop:bg-slate-900/40"
    >
      <form onSubmit={handleSubmit} noValidate className="space-y-4 p-6">
        <h2 className="text-lg font-semibold text-slate-900">口座を編集</h2>

        {!hasFieldError && <FormError error={error} />}

        {noChange && (
          <div
            role="alert"
            className="rounded-md border border-amber-200 bg-amber-50 px-3 py-2 text-sm text-amber-800"
          >
            変更された項目がありません。
          </div>
        )}

        <Field
          label="口座名"
          name="edit-account-name"
          value={values.name}
          onChange={(name) => setValues({ ...values, name })}
          error={nameError}
        />

        <div className="space-y-1">
          <span className="block text-sm font-medium text-slate-700">種別</span>
          <p className="rounded-md border border-slate-200 bg-slate-50 px-3 py-2 text-slate-500">
            {accountTypeLabels[account.account_type]}
          </p>
          <p className="text-sm text-slate-500">種別は変更できません。</p>
        </div>

        {isTokutei && (
          <fieldset className="space-y-1">
            <legend className="block text-sm font-medium text-slate-700">
              源泉徴収
            </legend>
            <div className="flex gap-4 pt-1">
              {[
                { value: true, label: "あり" },
                { value: false, label: "なし" },
              ].map((o) => (
                <label
                  key={String(o.value)}
                  className="flex items-center gap-2 text-sm text-slate-900"
                >
                  <input
                    type="radio"
                    name="edit-withholding"
                    checked={values.withholding === o.value}
                    onChange={() =>
                      setValues({ ...values, withholding: o.value })
                    }
                  />
                  {o.label}
                </label>
              ))}
            </div>
            {fieldErrors.withholding && (
              <p className="text-sm text-red-600">{fieldErrors.withholding}</p>
            )}
          </fieldset>
        )}

        <Field
          label="金融機関名"
          name="edit-institution"
          value={values.institution}
          onChange={(institution) => setValues({ ...values, institution })}
          error={fieldErrors.institution}
          hint="空欄にすると登録済みの金融機関名を削除します。"
        />

        <div className="space-y-1">
          <span className="block text-sm font-medium text-slate-700">通貨</span>
          <p className="rounded-md border border-slate-200 bg-slate-50 px-3 py-2 text-slate-500">
            {account.currency}
          </p>
          <p className="text-sm text-slate-500">通貨は変更できません。</p>
        </div>

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
    </dialog>
  );
}
