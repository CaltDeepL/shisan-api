import { useEffect, useRef, useState } from "react";
import { ApiError } from "@/api/problem";
import { DialogShell } from "@/components/DialogShell";
import { Field } from "@/components/Field";
import { FormError } from "@/components/FormError";
import type { AccountType, CreateAccountRequest } from "@/api/accounts";
import { useCreateAccount } from "./queries";
import { accountTypeLabels, accountTypeOptions } from "./labels";

type Props = {
  open: boolean;
  onClose: () => void;
};

const initialValues = {
  name: "",
  accountType: "tokutei" as AccountType,
  withholding: true,
  institution: "",
  currency: "JPY",
};

export function CreateAccountDialog({ open, onClose }: Props) {
  const ref = useRef<HTMLDialogElement>(null);
  const [values, setValues] = useState(initialValues);
  const [error, setError] = useState<unknown>(null);
  const create = useCreateAccount();

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

  // 409 は errors[] を持たないので、口座名の重複として自前で紐づける
  const nameError =
    apiError?.status === 409
      ? "同じ名前の口座が既に登録されています"
      : fieldErrors.name;

  // フィールドに出せたものは上段に重複表示しない
  const hasFieldError =
    apiError?.status === 409 || Object.keys(fieldErrors).length > 0;

  const isTokutei = values.accountType === "tokutei";

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setError(null);

    const body: CreateAccountRequest = {
      name: values.name.trim(),
      account_type: values.accountType,
      currency: values.currency.trim().toUpperCase(),
      // 特定口座以外は NULL でないと accounts_withholding_only_tokutei に弾かれる
      withholding: isTokutei ? values.withholding : null,
      institution: values.institution.trim() || null,
    };

    try {
      await create.mutateAsync(body);
      onClose();
    } catch (e) {
      setError(e);
    }
  }

  return (
    <DialogShell dialogRef={ref} onClose={onClose}>
      <form onSubmit={handleSubmit} noValidate className="space-y-4 p-6">
        <h2 className="text-lg font-semibold text-slate-900">口座を追加</h2>

        {!hasFieldError && <FormError error={error} />}

        <Field
          label="口座名"
          name="account-name"
          value={values.name}
          onChange={(name) => setValues({ ...values, name })}
          error={nameError}
        />

        <div className="space-y-1">
          <label
            htmlFor="account-type"
            className="block text-sm font-medium text-slate-700"
          >
            種別
          </label>
          <select
            id="account-type"
            name="account-type"
            value={values.accountType}
            onChange={(e) =>
              setValues({
                ...values,
                accountType: e.target.value as AccountType,
              })
            }
            className="w-full rounded-md border border-slate-300 px-3 py-2 text-slate-900 outline-none focus:ring-2 focus:ring-slate-300"
          >
            {accountTypeOptions.map((t) => (
              <option key={t} value={t}>
                {accountTypeLabels[t]}
              </option>
            ))}
          </select>
          <p className="text-sm text-slate-500">作成後は変更できません。</p>
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
                    name="withholding"
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
          name="institution"
          value={values.institution}
          onChange={(institution) => setValues({ ...values, institution })}
          error={fieldErrors.institution}
          hint="任意。空欄のままでも登録できます。"
        />

        <Field
          label="通貨"
          name="currency"
          value={values.currency}
          onChange={(currency) => setValues({ ...values, currency })}
          error={fieldErrors.currency}
          hint="3文字の通貨コード。作成後は変更できません。"
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
    </DialogShell>
  );
}
