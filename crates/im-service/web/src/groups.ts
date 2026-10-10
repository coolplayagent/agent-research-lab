import { change } from "./api.js";
import { close, get, notice, open, value } from "./dom.js";
import { select } from "./conversation.js";
import { loadGroups } from "./directory.js";
import { Picker } from "./picker.js";
import type { Group, Kind } from "./types.js";
export function initGroups(): void {
  const picker = new Picker(get("create-picker"));
  const kind = get<HTMLSelectElement>("group-kind");
  const updateKind = (): void => {
    const temporary = kind.value === "temporary";
    get<HTMLInputElement>("group-name").required = !temporary;
    get("group-name-label").hidden = temporary;
    get("group-kind-note").textContent = temporary
      ? "选择参与者即可开始。适合临时讨论，可以随时转为正式群组。"
      : kind.value === "board"
        ? "管理员发布公告，成员可以订阅和查阅消息。"
        : "按团队或主题组织 Agent，成员共享群组历史。";
  };
  kind.onchange = updateKind;
  get("new-conversation").onclick = () => {
    get<HTMLFormElement>("group-form").reset();
    kind.value =
      location.hash === "#boards"
        ? "board"
        : location.hash === "#direct"
          ? "temporary"
          : "conversation";
    updateKind();
    picker.reset();
    notice("", "create-status");
    open("group-dialog");
  };
  get<HTMLFormElement>("group-form").onsubmit = async (event) => {
    event.preventDefault();
    get<HTMLButtonElement>("create-submit").disabled = true;
    try {
      if (!picker.chosen.size) throw new Error("请选择至少一位参与者。");
      const names = [...picker.chosen.values()].map((p) => p.name).join("、");
      const temporary = kind.value === "temporary";
      const result = await change<Group>({
        operation: "create",
        group: {
          id: "room-" + crypto.randomUUID(),
          kind: kind.value as Kind,
          private: temporary,
          title: temporary ? names.slice(0, 70) : value("group-name"),
          topic:
            value("group-description") ||
            (temporary
              ? "临时多人讨论"
              : kind.value === "board"
                ? "共享公告与团队进展"
                : "共同讨论与协作"),
          members: [...picker.chosen.keys()],
        },
      });
      close("group-dialog");
      location.hash =
        kind.value === "board" ? "#boards" : temporary ? "#direct" : "#groups";
      await loadGroups();
      await select(result);
    } catch (error) {
      notice(error, "create-status");
    } finally {
      get<HTMLButtonElement>("create-submit").disabled = false;
    }
  };
}
