export type FieldError = { field: string; message: string };

export type ProblemDetails = {
  /** サーバーからの実レスポンス以外（通信エラー・本文なし等）では省略される */
  type?: string;
  title: string;
  status?: number;
  detail?: string;
  /** 空のときはサーバー側で省略される */
  errors?: FieldError[];
  trace_id?: string;
};

/** API が返したエラーを表す例外。status 0 はネットワーク到達不能。 */
export class ApiError extends Error {
  readonly status: number;
  readonly problem: ProblemDetails;

  constructor(status: number, problem: ProblemDetails) {
    super(problem.detail ?? problem.title ?? `HTTP ${status}`);
    this.name = "ApiError";
    this.status = status;
    this.problem = problem;
  }

  /** 422 の errors を field 名で引ける形に畳む。同一 field は最初の1件を採る。 */
  get fieldErrors(): Record<string, string> {
    const out: Record<string, string> = {};
    for (const e of this.problem.errors ?? []) {
      if (!(e.field in out)) out[e.field] = e.message;
    }
    return out;
  }
}
