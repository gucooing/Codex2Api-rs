import type { Supplier } from "./api";

export function toggleSupplierSelection(selected: string[], ids: string[], checked: boolean) {
  const next = new Set(selected);
  for (const id of ids) {
    if (checked) next.add(id);
    else next.delete(id);
  }
  return [...next];
}

export function selectedSupplierProvider(
  accounts: Pick<Supplier, "provider_id">[],
): string | undefined {
  const providers = new Set(accounts.map((account) => account.provider_id));
  return providers.size === 1 ? [...providers][0] : undefined;
}

export function commonSupplierTags(accounts: Pick<Supplier, "tag_ids">[]): string[] {
  return (accounts[0]?.tag_ids ?? []).filter((id) =>
    accounts.every((account) => account.tag_ids.includes(id)),
  );
}

export function supplierTagChecked(
  accounts: Pick<Supplier, "tag_ids">[],
  tag: string,
): boolean | "indeterminate" {
  const count = accounts.filter((account) => account.tag_ids.includes(tag)).length;
  return count === 0 ? false : count === accounts.length ? true : "indeterminate";
}
