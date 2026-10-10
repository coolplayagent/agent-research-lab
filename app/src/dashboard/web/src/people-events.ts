import { workspaceState } from "./state.js";
import { $ } from "./ui.js";
import {
  peopleApi,
  renderPeopleDirectory,
  loadPeople,
  closePerson,
  settingsPeopleView,
} from "./people-directory.js";
import { refresh } from "./observer.js";
import {
  openPerson,
  recallPerson,
  switchPersonTab,
  editPerson,
  loadPersonHistory,
} from "./people-profile.js";
import { editPersonExecution, updateExecutionModels } from "./executors.js";
export function initialize(): void {
  $("person-form").addEventListener("submit", async (event?: any) => {
    event.preventDefault();
    $("save-person").disabled = true;
    const fields = {
      name: $("person-name").value,
      soul: $("person-soul").value,
      purpose: $("person-purpose").value,
    };
    const change = workspaceState.editingPerson
      ? {
          action: "update",
          id: workspaceState.editingPerson.id,
          revision: workspaceState.editingPerson.revision,
          ...fields,
        }
      : {
          action: "create",
          role: $("person-role").value,
          kind: $("person-kind").value,
          ...fields,
        };
    try {
      const result = await peopleApi({ operation: "profile", change });
      workspaceState.peopleIndex.set(result.person.id, result.person);
      $("person-form").hidden = true;
      renderPeopleDirectory();
      await loadPeople();
      await refresh();
      await openPerson(result.person.id);
    } catch (error) {
      $("person-form-status").textContent = error.message;
    } finally {
      $("save-person").disabled = false;
    }
  });
  $("person-note-form").addEventListener("submit", async (event?: any) => {
    event.preventDefault();
    const note = $("person-memory-note").value.trim();
    if (!note) return;
    const id = workspaceState.personFocus;
    workspaceState.memoryNoteRequest =
      workspaceState.memoryNoteRequest?.note === note &&
      workspaceState.memoryNoteRequest.id === id
        ? workspaceState.memoryNoteRequest
        : { id, note, key: crypto.randomUUID() };
    $("person-remember").disabled = true;
    try {
      await peopleApi({
        operation: "remember",
        id,
        request_id: workspaceState.memoryNoteRequest.key,
        note,
      });
      if (workspaceState.personFocus === id) {
        $("person-memory-note").value = "";
        workspaceState.memoryNoteRequest = null;
        await recallPerson();
      }
    } catch (error) {
      if (workspaceState.personFocus === id)
        $("person-status").textContent = error.message;
    } finally {
      $("person-remember").disabled = false;
    }
  });
  $("person-recall-form").addEventListener("submit", (event?: any) => {
    event.preventDefault();
    recallPerson();
  });
  $("person-tab-history").addEventListener("click", () =>
    switchPersonTab("history"),
  );
  $("person-tab-memory").addEventListener("click", () => {
    switchPersonTab("memory");
    recallPerson();
  });
  $("promote-person").addEventListener("click", async () => {
    const p = workspaceState.peopleIndex.get(workspaceState.personFocus);
    $("promote-person").disabled = true;
    try {
      const result = await peopleApi({
        operation: "profile",
        change: { action: "promote", id: p.id, revision: p.revision },
      });
      workspaceState.peopleIndex.set(p.id, result.person);
      await loadPeople();
      await refresh();
      await openPerson(p.id);
    } catch (error) {
      $("person-status").textContent = error.message;
    } finally {
      $("promote-person").disabled = false;
    }
  });
  $("edit-person").addEventListener("click", () =>
    editPerson(workspaceState.peopleIndex.get(workspaceState.personFocus)),
  );
  $("close-person").addEventListener("click", () => {
    const id = workspaceState.personFocus;
    closePerson();
    if (!$("settings-page").hidden) {
      const index = Array.from<any>($("people-directory").children).find(
        (card?: any) =>
          card.textContent.includes(workspaceState.peopleIndex.get(id)?.name),
      );
      (index || $("create-person")).focus({ preventScroll: true });
    }
  });
  $("person-window").addEventListener(
    "cancel",
    () => workspaceState.personRequest++,
  );
  $("person-history-more").addEventListener("click", () =>
    loadPersonHistory(workspaceState.personHistoryCursor),
  );
  $("create-person").addEventListener("click", () => editPerson());
  $("cancel-person").addEventListener("click", () => {
    if (workspaceState.editingPerson)
      openPerson(workspaceState.editingPerson.id);
    else {
      settingsPeopleView("list");
      $("create-person").focus();
    }
  });
  $("people-search").addEventListener("input", renderPeopleDirectory);
  $("people-temporary").addEventListener("change", renderPeopleDirectory);
  $("people-more").addEventListener("click", () =>
    loadPeople(workspaceState.peopleCursor),
  );
  $("nav-settings").addEventListener("click", () => loadPeople());
  document.addEventListener("keydown", (event?: any) => {
    if (
      event.key === "Escape" &&
      $("person-window").open &&
      $("person-window").classList.contains("embedded") &&
      !$("session-window").open
    ) {
      event.preventDefault();
      $("close-person").click();
    }
  });
  $("maximize-person").addEventListener("click", () => {
    const expanded = $("person-window").classList.toggle("expanded");
    $("maximize-person").textContent = expanded ? "↙ 还原" : "↗ 放大";
    $("maximize-person").setAttribute("aria-pressed", String(expanded));
    $("maximize-person").setAttribute(
      "aria-label",
      `${expanded ? "还原" : "放大"}数字人窗口`,
    );
  });
  $("person-tab-execution").addEventListener("click", () => {
    switchPersonTab("execution");
    editPersonExecution();
  });
  $("person-execution-form").addEventListener("submit", async (event?: any) => {
    event.preventDefault();
    const person = workspaceState.executionEditor,
      request = workspaceState.personRequest;
    if (!person || person.id !== workspaceState.personFocus) return;
    const backend = $("person-backend").value || null,
      model = $("person-model").value || null;
    $("save-person-execution").disabled = true;
    try {
      const result = await peopleApi({
        operation: "profile",
        change: {
          action: "configure_execution",
          id: person.id,
          revision: person.revision,
          execution: backend || model ? { backend, model } : null,
        },
      });
      workspaceState.peopleIndex.set(person.id, result.person);
      workspaceState.executionEditor = result.person;
      await loadPeople();
      await refresh();
      if (
        workspaceState.personFocus === person.id &&
        workspaceState.personRequest === request
      ) {
        $("person-execution-status").textContent =
          "已保存，新任务将使用此偏好。已有会话、记忆和执行记录保持不变。";
        $("person-subtitle").textContent =
          `${workspaceState.personKinds[result.person.kind]} · ${workspaceState.personRoles[result.person.role] || result.person.role} · 执行偏好：${backend || "按任务选择"} / ${model || "按任务选择模型"}`;
      }
    } catch (error) {
      if (
        workspaceState.personFocus === person.id &&
        workspaceState.personRequest === request
      )
        $("person-execution-status").textContent = error.message;
    } finally {
      $("save-person-execution").disabled = false;
    }
  });
  $("person-backend").addEventListener("change", updateExecutionModels);
}
