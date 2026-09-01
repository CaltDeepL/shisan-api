import type { components } from "./schema";

export type FieldError = components["schemas"]["FieldError"];
export type ProblemDetails = Partial<components["schemas"]["ProblemDetails"]>;

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