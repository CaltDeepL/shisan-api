import { apiFetch } from "./client";
import type { components } from "./schema";

export type Asset = components["schemas"]["AssetResponse"];
export type AssetClass = components["schemas"]["AssetClass"];
export type CreateAssetRequest = components["schemas"]["CreateAssetRequest"];
export type PatchAssetRequest = components["schemas"]["PatchAssetRequest"];
export type PriceItem = components["schemas"]["PriceItem"];
export type PriceResponse = components["schemas"]["PriceResponse"];
export type UpsertPricesRequest =
  components["schemas"]["UpsertPricesRequest"];
export type UpsertPricesResponse =
  components["schemas"]["UpsertPricesResponse"];

export function listPrices(assetId: string): Promise<PriceResponse[]> {
  return apiFetch<PriceResponse[]>(`/prices/${assetId}`);
}

export function upsertPrices(
  body: UpsertPricesRequest,
): Promise<UpsertPricesResponse> {
  return apiFetch<UpsertPricesResponse>("/prices", { method: "POST", body });
}

export function listAssets(q: string): Promise<Asset[]> {
  const trimmed = q.trim();
  const query = trimmed ? `?q=${encodeURIComponent(trimmed)}` : "";
  return apiFetch<Asset[]>(`/assets${query}`);
}

export function createAsset(body: CreateAssetRequest): Promise<Asset> {
  return apiFetch<Asset>("/assets", { method: "POST", body });
}

export function patchAsset(
  id: string,
  body: PatchAssetRequest,
): Promise<Asset> {
  return apiFetch<Asset>(`/assets/${id}`, { method: "PATCH", body });
}

/** 作成フォームの入力値。price_unit は空欄可（サーバー既定に委ねる）。 */
export type CreateAssetFormValues = {
  symbol: string;
  name: string;
  asset_class: AssetClass;
  currency: string;
  price_unit: string;
};

export function buildCreateAsset(
  values: CreateAssetFormValues,
): CreateAssetRequest {
  const body: CreateAssetRequest = {
    symbol: values.symbol.trim(),
    name: values.name.trim(),
    asset_class: values.asset_class,
  };
  // currency は未指定ならサーバー既定（JPY）。空欄でキーを送らない
  const currency = values.currency.trim();
  if (currency !== "") body.currency = currency;
  // price_unit は未指定なら資産クラス別の既定（投信 10000 / 他 1）。
  // フロントで補うと既定のロジックが二重定義になるのでキーごと省く
  const priceUnit = values.price_unit.trim();
  if (priceUnit !== "") body.price_unit = priceUnit;
  return body;
}

/** 編集フォームの入力値。asset_class と currency は変更不可なので持たない。 */
export type PatchAssetFormValues = {
  symbol: string;
  name: string;
  price_unit: string;
};

export function buildAssetPatch(
  before: Asset,
  values: PatchAssetFormValues,
): PatchAssetRequest | null {
  const patch: PatchAssetRequest = {};

  const symbol = values.symbol.trim();
  if (symbol !== "" && symbol !== before.symbol) patch.symbol = symbol;

  const name = values.name.trim();
  if (name !== "" && name !== before.name) patch.name = name;

  // price_unit は numeric(12,0) の整数が文字列で返る。
  // "1" と "01" を別物と扱わないよう数値で比較する
  const priceUnit = values.price_unit.trim();
  if (priceUnit !== "" && Number(priceUnit) !== Number(before.price_unit)) {
    patch.price_unit = priceUnit;
  }

  return Object.keys(patch).length === 0 ? null : patch;
}