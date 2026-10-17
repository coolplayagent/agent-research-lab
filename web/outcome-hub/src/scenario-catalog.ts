import { api } from "./api.js";
import type { Scenario } from "./goal-types.js";
const descriptors = new Map<string, Scenario>();
let revision = 0;
export async function loadScenarioCatalog(): Promise<void> {
  const request = ++revision;
  try {
    const { scenarios } = await api<{ scenarios: Scenario[] }>("/api/v1/collaboration/scenarios");
    if (request !== revision) return;
    descriptors.clear();
    for (const scenario of scenarios) descriptors.set(scenario.id, scenario);
  } catch { if (request === revision) descriptors.clear(); }
}
export function scenarioDescriptor(id: string): Scenario | undefined { return descriptors.get(id); }
