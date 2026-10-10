import { api, change, query } from "./api.js";
import { current, refreshCurrent, select } from "./conversation.js";
import {
  button,
  close,
  el,
  get,
  notice,
  open,
  presence,
  value,
} from "./dom.js";
import { loadGroups } from "./directory.js";
import { display } from "./people.js";
import { Picker } from "./picker.js";
import type { Group, GroupView } from "./types.js";
let selected: Group | null = null,
  next: string | null = null,
  generation = 0;
export function initMembers(): void {
  const picker = new Picker(get("member-picker"));
  async function load(append = false): Promise<void> {
    if (!selected) return;
    const request = ++generation,
      id = selected.id;
    try {
      const page = await api<GroupView>(
        "/api/crystal/view?" +
          query({ group_id: id, after: append ? next || "" : "" }),
      );
      if (request !== generation) return;
      selected = page.group;
      const root = get("member-list");
      if (!append) root.replaceChildren();
      next = page.members.next_after;
      get("more-members").hidden = !next;
      for (const member of page.members.members) {
        picker.excluded.add(member.person_id);
        const row = el("div", undefined, "member");
        row.append(
          el("span", display(member.person_id)),
          el(
            "small",
            member.person_id === "operator"
              ? "管理员"
              : presence[member.presence],
          ),
        );
        if (
          member.person_id !== "operator" &&
          !selected.archived &&
          !selected.private
        )
          row.append(
            button("移除", async () => {
              try {
                await change({
                  operation: "membership",
                  group_id: id,
                  person_id: member.person_id,
                  add: false,
                });
                await load();
                await refreshCurrent();
              } catch (error) {
                notice(error, "member-status");
              }
            }),
          );
        root.append(row);
      }
      get("add-members-title").textContent = selected.private
        ? "邀请更多人，开始新的多人私信"
        : "添加成员";
      get("history-choice").hidden = !selected.private;
      get("add-members").textContent = selected.private
        ? "创建新会话"
        : "添加所选成员";
      get<HTMLButtonElement>("add-members").disabled = selected.archived;
      await picker.load();
    } catch (error) {
      notice(error, "member-status");
    }
  }
  get("manage-members").onclick = () => {
    if (!current) return;
    selected = { ...current };
    picker.reset();
    notice("", "member-status");
    get<HTMLSelectElement>("include-history").value = "none";
    open("members-dialog");
    void load();
  };
  get("more-members").onclick = () => void load(true);
  get("add-members").onclick = async () => {
    if (!selected) return;
    get<HTMLButtonElement>("add-members").disabled = true;
    try {
      if (!picker.chosen.size) throw new Error("请选择要加入的 Agent。");
      const group = selected;
      if (group.private) {
        const existing = new Set<string>();
        let after = "";
        do {
          const page = await api<GroupView>(
            "/api/crystal/view?" + query({ group_id: group.id, after }),
          );
          page.members.members.forEach((m) => existing.add(m.person_id));
          after = page.members.next_after || "";
        } while (after);
        picker.chosen.forEach((_, id) => existing.add(id));
        const result = await change<Group>({
          operation: "fork",
          source: group.id,
          history_from: value("include-history") === "all" ? 0 : null,
          group: {
            id: "dm-" + crypto.randomUUID(),
            title:
              `${group.title}、${[...picker.chosen.values()].map((p) => p.name).join("、")}`.slice(
                0,
                70,
              ),
            topic: group.topic,
            kind: "temporary",
            private: true,
            members: [...existing],
          },
        });
        close("members-dialog");
        await loadGroups();
        await select(result);
      } else {
        for (const person_id of [...picker.chosen.keys()]) {
          await change({
            operation: "membership",
            group_id: group.id,
            person_id,
            add: true,
          });
          picker.chosen.delete(person_id);
        }
        notice("成员已添加。", "member-status");
        await load();
        await refreshCurrent();
      }
    } catch (error) {
      notice(error, "member-status");
    } finally {
      get<HTMLButtonElement>("add-members").disabled =
        selected?.archived ?? true;
    }
  };
}
