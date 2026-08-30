import { ApiError } from "@/api/problem";

export function FormError({ error }: { error: unknown }) {
  if (!error) return null;

  const isApiError = error instanceof ApiError;
  const message = isApiError ? error.message : "予期しないエラーが発生しました";

  // 422 はフィールド側に出しているので、ここでは本文だけ出す
  const traceId =
    isApiError && error.status >= 500 ? error.problem.trace_id : null;

  return (
    <div
      role="alert"
      className="rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700"
    >
      <p>{message}</p>
      {traceId && (
        <p className="mt-1 font-mono text-xs text-red-500">ID: {traceId}</p>
      )}
    </div>
  );
}
