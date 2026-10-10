import { GroupKind } from "/assets/shared/contracts.js";
import { t } from "./i18n.js";
import { change } from "./api.js";
import { close, get, notice, open, value } from "./dom.js";
import { select } from "./conversation.js";
import { loadGroups } from "./directory.js";
import { Picker } from "./picker.js";
export function initGroups() {
    const picker = new Picker(get("create-picker"));
    const kind = get("group-kind");
    const updateKind = () => {
        const temporary = kind.value === GroupKind.Temporary;
        get("group-name").required = !temporary;
        get("group-name-label").hidden = temporary;
        get("group-kind-note").textContent = temporary
            ? t("groups.570d94d691")
            : kind.value === GroupKind.Board
                ? t("groups.9b7278eca3")
                : t("groups.73a02c2189");
    };
    kind.onchange = updateKind;
    get("new-conversation").onclick = () => {
        get("group-form").reset();
        kind.value =
            location.hash === "#boards"
                ? GroupKind.Board
                : location.hash === "#direct"
                    ? GroupKind.Temporary
                    : GroupKind.Conversation;
        updateKind();
        picker.reset();
        notice("", "create-status");
        open("group-dialog");
    };
    get("group-form").onsubmit = async (event) => {
        event.preventDefault();
        get("create-submit").disabled = true;
        try {
            if (!picker.chosen.size)
                throw new Error(t("groups.64919f14f1"));
            const names = [...picker.chosen.values()].map((p) => p.name).join("、");
            const temporary = kind.value === GroupKind.Temporary;
            const result = await change({
                operation: "create",
                group: {
                    id: "room-" + crypto.randomUUID(),
                    kind: kind.value,
                    private: temporary,
                    title: temporary ? names.slice(0, 70) : value("group-name"),
                    topic: value("group-description") ||
                        (temporary
                            ? t("groups.90752e1a59")
                            : kind.value === GroupKind.Board
                                ? t("groups.1f519ce823")
                                : t("groups.0224052fc0")),
                    members: [...picker.chosen.keys()],
                },
            });
            close("group-dialog");
            location.hash =
                kind.value === GroupKind.Board
                    ? "#boards"
                    : temporary
                        ? "#direct"
                        : "#groups";
            await loadGroups();
            await select(result);
        }
        catch (error) {
            notice(error, "create-status");
        }
        finally {
            get("create-submit").disabled = false;
        }
    };
}
