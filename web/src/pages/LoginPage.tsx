import { useState } from "react";
import { Link } from "react-router";
import { useMutation } from "@tanstack/react-query";
import { login } from "@/api/auth";
import { useAuthStore } from "@/stores/auth";
import { Field } from "@/components/Field";
import { FormError } from "@/components/FormError";

export function LoginPage() {
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const setSession = useAuthStore((s) => s.setSession);

  const mutation = useMutation({
    mutationFn: login,
    onSuccess: (res) => setSession(res),
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    mutation.mutate({ email, password });
  };

  return (
    <div className="mx-auto mt-24 max-w-sm space-y-6 px-4">
      <h1 className="text-2xl font-semibold text-slate-900">ログイン</h1>

      <form onSubmit={handleSubmit} className="space-y-4">
        <FormError error={mutation.error} />

        <Field
          label="メールアドレス"
          name="email"
          type="email"
          value={email}
          onChange={setEmail}
          autoComplete="email"
        />
        <Field
          label="パスワード"
          name="password"
          type="password"
          value={password}
          onChange={setPassword}
          autoComplete="current-password"
        />

        <button
          type="submit"
          disabled={mutation.isPending}
          className="w-full rounded-md bg-slate-900 py-2 text-white disabled:opacity-50"
        >
          {mutation.isPending ? "送信中…" : "ログイン"}
        </button>
      </form>

      <p className="text-sm text-slate-600">
        アカウントをお持ちでない方は{" "}
        <Link to="/register" className="underline">
          新規登録
        </Link>
      </p>
    </div>
  );
}