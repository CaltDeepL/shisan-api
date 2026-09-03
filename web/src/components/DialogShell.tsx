import type { ReactNode, Ref } from "react";

type Props = {
  dialogRef: Ref<HTMLDialogElement>;
  onClose: () => void;
  /** PriceDialog のように内容が広い場合は "lg" */
  size?: "md" | "lg";
  children: ReactNode;
};

/** 全ダイアログ共通の `<dialog>` 要素。開閉のライフサイクルは呼び出し側が持つ。 */
export function DialogShell({ dialogRef, onClose, size = "md", children }: Props) {
  return (
    <dialog
      ref={dialogRef}
      onCancel={onClose}
      onClose={onClose}
      className={`w-full ${size === "lg" ? "max-w-lg" : "max-w-md"} rounded-lg p-0 backdrop:bg-slate-900/40`}
    >
      {children}
    </dialog>
  );
}
