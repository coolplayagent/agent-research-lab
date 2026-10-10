import { GroupKind } from "/assets/shared/contracts.js";
import { t } from "./i18n.js";
import { label } from "./i18n.js";
import { api } from "./api.js";
import { button, el, get, notice } from "./dom.js";
import { current } from "./conversation.js";
import { openGoal } from "./goal-form.js";
import type { Scenario } from "./goal-types.js";
export async function loadApplications(): Promise<void> {
  try {
    const { scenarios, status } = await api<{
      scenarios: Scenario[];
      status: Record<
        string,
        {
          status?: string;
        }
      >;
    }>("/api/im/scenarios");
    const root = get("application-list");
    root.replaceChildren();
    for (const scenario of scenarios) {
      const card = el("article", undefined, "card");
      card.append(
        el("h3", label("Scenario", scenario.id)),
        el("p", label("ScenarioDescription", scenario.id)),
        el(
          "small",
          label("ScenarioStatus", status[scenario.id]?.status || "") ||
            t("applications.dbf6c16b45"),
        ),
        button(t("applications.eb44dbca1f"), () => {
          location.hash = "#groups";
          if (current && current.kind !== GroupKind.Board && !current.archived)
            void openGoal(scenario.id);
          else notice(t("applications.4d4a0fe582"));
        }),
      );
      root.append(card);
    }
  } catch (error) {
    notice(error);
  }
}
