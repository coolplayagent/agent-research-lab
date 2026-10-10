import { api } from "./api.js";
import { button, el, get, notice } from "./dom.js";
import { current } from "./conversation.js";
import { openGoal } from "./goal-form.js";
import type { Scenario } from "./goal-types.js";
export async function loadApplications(): Promise<void> {
  try {
    const { scenarios, status } = await api<{
      scenarios: Scenario[];
      status: Record<string, { message?: string }>;
    }>("/api/im/scenarios");
    const root = get("application-list");
    root.replaceChildren();
    for (const scenario of scenarios) {
      const card = el("article", undefined, "card");
      card.append(
        el("h3", scenario.name),
        el("p", scenario.description),
        el(
          "small",
          status[scenario.id]?.message || "等待接入的 Agent 领取任务",
        ),
        button("在群组中设置目标", () => {
          location.hash = "#groups";
          if (current && current.kind !== "board" && !current.archived)
            void openGoal(scenario.id);
          else notice("先选择或创建群组，再设置目标与成员分工。");
        }),
      );
      root.append(card);
    }
  } catch (error) {
    notice(error);
  }
}
