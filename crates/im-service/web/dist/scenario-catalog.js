import { api } from "./api.js";
const descriptors = new Map();
let revision = 0;
export async function loadScenarioCatalog() {
    const request = ++revision;
    try {
        const { scenarios } = await api("/api/im/scenarios");
        if (request !== revision)
            return;
        descriptors.clear();
        for (const scenario of scenarios)
            descriptors.set(scenario.id, scenario);
    }
    catch {
        if (request === revision)
            descriptors.clear();
    }
}
export function scenarioDescriptor(id) { return descriptors.get(id); }
