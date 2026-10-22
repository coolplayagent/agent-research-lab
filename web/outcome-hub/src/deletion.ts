import { DeletionCode } from "/assets/shared/contracts.js";
import { change } from "./api.js";
import { t } from "./i18n.js";
export async function deleteObject(target: object, title: string): Promise<boolean> {
  if (!confirm(t("deletion.confirm", { title }))) return false;
  const result = await change<{ code: DeletionCode; references: string[] }>({ operation: "delete", target });
  if (result.code !== DeletionCode.Deleted) throw new Error(t("DeletionCode." + result.code) + (result.references.length ? "\n" + result.references.join(", ") : ""));
  return true;
}
