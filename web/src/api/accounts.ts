import { apiFetch } from "./client";
import type { components } from "./schema";

export type Account = components["schemas"]["AccountResponse"];
export type AccountType = components["schemas"]["AccountType"];
export type CreateAccountRequest =
  components["schemas"]["CreateAccountRequest"];
export type UpdateAccountRequest =
  components["schemas"]["UpdateAccountRequest"];

export function listAccounts(): Promise<Account[]> {
  return apiFetch<Account[]>("/accounts");
}

export function createAccount(body: CreateAccountRequest): Promise<Account> {
  return apiFetch<Account>("/accounts", { method: "POST", body });
}

export function updateAccount(
  id: string,
  body: UpdateAccountRequest,
): Promise<Account> {
  return apiFetch<Account>(`/accounts/${id}`, { method: "PATCH", body });
}

export function deleteAccount(id: string): Promise<void> {
  return apiFetch<void>(`/accounts/${id}`, { method: "DELETE" });
}

/** 編集フォームの入力値。account_type と currency は変更不可なので持たない。 */
export type AccountFormValues = {
  name: string;
  institution: string;
  /** account_type が tokutei のときのみ意味を持つ */
  withholding: boolean | null;
};

/**
 * 現在値との差分だけを含む PATCH ボディを組み立てる。
 * 変更が1つも無ければ null を返す（そのまま送るとサーバーが 400 を返すため）。
 */
export function buildAccountPatch(
  before: Account,
  values: AccountFormValues,
): UpdateAccountRequest | null {
  const patch: UpdateAccountRequest = {};

  // name は Option<String>。null は「変更なし」と解釈されるので、空文字は送らない
  const name = values.name.trim();
  if (name !== "" && name !== before.name) patch.name = name;

  // institution は空欄を null に倒してクリア扱いにする
  const institution = values.institution.trim() || null;
  if (institution !== (before.institution ?? null))
    patch.institution = institution;

  // withholding は tokutei のときだけ送る。
  // それ以外の口座では常に NULL でなければならず、変更する余地がない
  if (before.account_type === "tokutei" && values.withholding !== null) {
    if (values.withholding !== before.withholding) {
      patch.withholding = values.withholding;
    }
  }

  return Object.keys(patch).length === 0 ? null : patch;
}
