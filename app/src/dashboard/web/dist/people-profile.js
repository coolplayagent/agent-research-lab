import { t } from "./i18n.js";
import { label } from "./i18n.js";
import { workspaceState } from "./state.js";
import { closePerson, settingsPeopleView, loadPeople, personPresence, personRepresentative, peopleApi, } from "./people-directory.js";
import { showPage } from "./navigation.js";
import { $, el, badge, messageTimestamp } from "./ui.js";
import { closeSession, openSessionRecord } from "./sessions.js";
import { avatar, readable, fold } from "./identity.js";
export function editPerson(person = null) {
    closePerson();
    location.hash = "settings/people";
    showPage("settings/people", true);
    workspaceState.editingPerson = person;
    settingsPeopleView("form");
    $("person-form-title").textContent = person
        ? t("people-profile.8865ac1c54", { p0: person.name })
        : t("people-profile.e47aec6e4e");
    $("person-name").value = person?.name || "";
    $("person-purpose").value = person?.purpose || "";
    $("person-soul").value = person?.soul || "";
    $("person-kind").value = person?.kind || "fixed";
    $("person-kind").disabled = Boolean(person);
    const roles = $("person-role");
    roles.replaceChildren();
    const options = new Set([
        ...Object.keys(workspaceState.peopleModels),
        ...(workspaceState.peopleModels.research ? ["synthesis"] : []),
    ]);
    if (person)
        options.add(person.role);
    for (const role of options) {
        const option = el("option", workspaceState.personRoles[role] || role);
        option.value = role;
        roles.append(option);
    }
    roles.value = person?.role || "research";
    roles.disabled = Boolean(person);
    $("person-form-status").textContent = "";
    $("person-name").focus();
}
export async function openPerson(id) {
    let person = workspaceState.peopleIndex.get(id);
    if (!person) {
        await loadPeople();
        person = workspaceState.peopleIndex.get(id);
    }
    if (!person)
        return;
    workspaceState.personFocus = id;
    workspaceState.personRequest++;
    workspaceState.personHistory = [];
    workspaceState.personHistoryCursor = null;
    workspaceState.memoryNoteRequest = null;
    if ($("session-window").open)
        closeSession();
    $("person-title").textContent = person.name;
    $("person-subtitle").textContent =
        `${label("PersonKind", person.kind)} · ${workspaceState.personRoles[person.role] || person.role} · ${label("Presence", personPresence(id))}`;
    $("person-subtitle").textContent += t("people-profile.bfc561b34f", {
        p0: person.execution?.backend || t("people-profile.634d99415a"),
        p1: person.execution?.model || t("people-profile.65bc196425"),
    });
    $("person-avatar").replaceChildren(avatar(personRepresentative(person)));
    $("person-soul-view").textContent =
        person.soul || t("people-profile.3656fa477b");
    $("person-purpose-view").textContent = person.purpose;
    $("promote-person").hidden = person.kind === "fixed";
    $("person-status").textContent = "";
    $("person-memory-note").value = "";
    $("person-memory-query").value = "";
    $("person-memory-events").replaceChildren();
    $("person-memory-stats").textContent = t("people-profile.3a1aa40c43");
    switchPersonTab("history");
    const panel = $("person-window"), embedded = !$("settings-page").hidden &&
        workspaceState.settingsCategory === "people";
    if (panel.open && panel.classList.contains("embedded") !== embedded)
        panel.close();
    panel.classList.toggle("embedded", embedded);
    $("maximize-person").hidden = embedded;
    $("close-person").textContent = embedded
        ? t("people-profile.773a7a5f36")
        : "×";
    $("close-person").setAttribute("aria-label", embedded ? t("people-profile.24308c1b13") : t("people-profile.693a4aec52"));
    if (embedded) {
        settingsPeopleView("detail");
        if (panel.parentElement !== $("person-inline-host"))
            $("person-inline-host").append(panel);
        if (!panel.open)
            panel.show();
        $("close-person").focus({ preventScroll: true });
    }
    else {
        if (panel.parentElement !== document.body)
            document.body.append(panel);
        if (!panel.open)
            panel.showModal();
    }
    await loadPersonHistory();
}
export async function loadPersonHistory(after = "") {
    const id = workspaceState.personFocus, request = workspaceState.personRequest;
    $("person-status").textContent = t("people-profile.eb11f9cc41");
    $("person-history-more").disabled = true;
    try {
        const response = await fetch(`/api/people/${encodeURIComponent(id)}${after ? `?after=${encodeURIComponent(after)}` : ""}`, { signal: AbortSignal.timeout(10000) });
        if (!response.ok)
            throw new Error(t("people-profile.02889d8bb6"));
        const value = await response.json();
        if (workspaceState.personFocus !== id ||
            workspaceState.personRequest !== request)
            return;
        workspaceState.peopleIndex.set(id, value.person);
        workspaceState.personHistory.push(...value.sessions);
        workspaceState.personHistoryCursor = value.next_after;
        const root = $("person-history-list");
        root.replaceChildren();
        for (const job of workspaceState.personHistory) {
            const card = el("button", undefined, "person-session"), text = el("span", job.topics?.join(" · ") || job.repository);
            text.append(el("small", t("people-profile.f217d2a4df", {
                p0: job.backend,
                p1: job.model,
                p2: job.attempt,
                p3: job.session_id || job.id,
            })));
            card.append(text, badge(job.state));
            card.addEventListener("click", () => {
                if (!$("person-window").classList.contains("embedded"))
                    $("person-window").close();
                openSessionRecord(job);
            });
            root.append(card);
        }
        if (!workspaceState.personHistory.length)
            root.append(el("p", t("people-profile.336e961396"), "empty"));
        $("person-status").textContent = t("people-profile.2a8119b3bd", {
            p0: workspaceState.personHistory.length,
        });
        $("person-history-more").hidden = !workspaceState.personHistoryCursor;
    }
    catch (error) {
        if (workspaceState.personFocus === id &&
            workspaceState.personRequest === request)
            $("person-status").textContent = error.message;
    }
    finally {
        $("person-history-more").disabled = false;
    }
}
export function switchPersonTab(tab) {
    for (const key of ["history", "memory", "execution"]) {
        $("person-" + key + "-panel").hidden = key !== tab;
        $("person-tab-" + key).classList.toggle("active", key === tab);
    }
}
export async function recallPerson() {
    const id = workspaceState.personFocus, request = workspaceState.personRequest;
    $("person-status").textContent = t("people-profile.f2e925879e");
    try {
        const result = await peopleApi({
            operation: "recall",
            id,
            query: $("person-memory-query").value,
        });
        if (id !== workspaceState.personFocus ||
            request !== workspaceState.personRequest)
            return;
        $("person-memory-stats").textContent = t("people-profile.cfde887ca6", {
            p0: result.stats.event_count,
            p1: result.stats.session_count,
            p2: result.stats.topic_count,
        });
        const root = $("person-memory-events");
        root.replaceChildren();
        for (const event of result.events) {
            const card = el("article", undefined, "memory-event");
            card.append((() => {
                const label = el("small");
                label.append(messageTimestamp(Number(event.at)), el("span", ` · ${event.session === "profile-notes" ? t("people-profile.891c388b24") : t("people-profile.f59e69859e")}${event.excerpt ? t("people-profile.f3a471d717") : ""}`));
                return label;
            })(), readable(event.response || event.summary || "", `memory:${event.id}`));
            if (event.metadata?.receipt_sha256) {
                const evidence = fold(`memory-evidence:${event.id}`, t("people-profile.723fa05fe3"), "protocol-detail");
                evidence.append(el("pre", JSON.stringify(event.metadata, null, 2)));
                card.append(evidence);
            }
            root.append(card);
        }
        if (!result.events.length)
            root.append(el("p", t("people-profile.c288a7d7d6"), "empty"));
        $("person-status").textContent = result.unconfirmed_writebacks
            ? t("people-profile.045e900c82", { p0: result.unconfirmed_writebacks })
            : t("people-profile.32fece2a5e");
    }
    catch (error) {
        if (id === workspaceState.personFocus &&
            request === workspaceState.personRequest)
            $("person-status").textContent = error.message;
    }
}
