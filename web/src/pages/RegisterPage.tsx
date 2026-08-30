import { useState } from "react";
import { Link } from "react-router";
import { useMutation } from "@tanstack/react-query";
import { register } from "@/api/auth";
import { ApiError } from "@/api/problem";
import { useAuthStore } from "@/stores/auth";
import { Field } from "@/components/Field";
import { FormError } from "@/components/FormError";

export function RegisterPage() {
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const setSession = useAuthStore((s) => s.setSession);

  const mutation = useMutation({
    mutationFn: register,
    onSuccess: (res) => setSession(res),
  });

  const error = mutation.error;
  const fieldErrors =
    error instanceof ApiError && error.status === 422 ? error.fieldErrors : {};
  // 422 はフィールド側に出すので、上部には出さない
  const formError =
    error instanceof ApiError && error.status === 422 ? null : error;

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    mutation.mutate({ email, password });
  };

  return (
    <div className="mx-auto mt-24 max-w-sm space-y-6 px-4">
      <h1 className="text-2xl font-semibold text-slate-900">新規登録</h1>

      <form onSubmit={handleSubmit} className="space-y-4">
        <FormError error={formError} />

        <Field
          label="メールアドレス"
          name="email"
          type="email"
          value={email}
          onChange={setEmail}
          error={fieldErrors.email}
          autoComplete="email"
        />
        <Field
          label="パスワード"
          name="password"
          type="password"
          value={password}
          onChange={setPassword}
          error={fieldErrors.password}
          hint="12文字以上"
          autoComplete="new-password"
        />

        <button
          type="submit"
          disabled={mutation.isPending}
          className="w-full rounded-md bg-slate-900 py-2 text-white disabled:opacity-50"
        >
          {mutation.isPending ? "送信中…" : "登録する"}
        </button>
      </form>

      <p className="text-sm text-slate-600">
        すでにアカウントをお持ちの方は{" "}
        <Link to="/login" className="underline">
          ログイン
        </Link>
      </p>
    </div>
  );
}
