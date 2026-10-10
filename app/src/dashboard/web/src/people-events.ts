import { t } from "./i18n.js";
import { label } from "./i18n.js";
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
    $("maximize-person").textContent = expanded
      ? t("people-events.01831fe8e3")
      : t("people-events.5a2a22c0d8");
    $("maximize-person").setAttribute("aria-pressed", String(expanded));
    $("maximize-person").setAttribute(
      "aria-label",
      t("people-events.c20269537e", {
        p0: expanded
          ? t("people-events.ddde089462")
          : t("people-events.80f8fbcfa0"),
      }),
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
        $("person-execution-status").textContent = t(
          "people-events.a8f69c4025",
        );
        $("person-subtitle").textContent = t("people-events.3e349f0f3d", {
          p0: label("PersonKind", result.person.kind),
          p1:
            workspaceState.personRoles[result.person.role] ||
            result.person.role,
          p2: backend || t("people-events.634d99415a"),
          p3: model || t("people-events.65bc196425"),
        });
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
