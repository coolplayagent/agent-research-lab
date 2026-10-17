import { workspaceState } from "./state.js";
import { $ } from "./ui.js";
import { loadLineage } from "./lineage.js";
export function initialize(): void {
  $("lineage-refresh").onclick = () => loadLineage();
  $("lineage-next").onclick = () =>
    loadLineage(workspaceState.lineageNext || "");
  loadLineage();
}
