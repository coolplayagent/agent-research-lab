import { label } from "./i18n.js";
import { t } from "./i18n.js";
import { describe } from "./i18n.js";
import { workspaceState } from "./state.js";
import { $, el, badge, short, time } from "./ui.js";
import { session, agentPresence, name, handle, presenceLabel, avatar, choose, loadProfile, readable, role, fold, } from "./identity.js";
import { dismissProfile, setFollowing } from "./preferences.js";
import { openPerson } from "./people-profile.js";
export function details(j) {
    const root = $("detail"), s = j && session(j), profile = j && workspaceState.profiles.get(j.run_id);
    const key = JSON.stringify([j, s?.bytes, profile, j && agentPresence(j)]);
    if (key === workspaceState.detailKey)
        return;
    workspaceState.detailKey = key;
    const scroll = root.parentElement.scrollTop;
    root.replaceChildren();
    if (!j) {
        root.append(el("div", t("profile.17f06fdfb0"), "empty"));
        return;
    }
    const hero = el("div", undefined, "persona-hero"), title = el("div");
    title.append(el("h2", name(j)), el("code", "@" + handle(j)), badge(j.state));
    title.append(el("span", presenceLabel(j), `im-status ${agentPresence(j).state}`));
    hero.append(avatar(j), title);
    root.append(hero, el("p", `${j.model} · ${j.backend}`, "persona-model"));
    if (agentPresence(j).pending)
        root.append(el("p", t("profile.b4c41c68b7", { p0: agentPresence(j).pending }), "process-meta"));
    const inspect = el("button", t("profile.33c7a8e250"), "inspect-process");
    inspect.addEventListener("click", () => {
        dismissProfile();
        if (j.cohort_id !== workspaceState.room) {
            workspaceState.room = j.cohort_id || "";
            setFollowing(false);
        }
        $("search").value = "";
        $("state").value = "";
        choose(j.id, true);
    });
    root.append(inspect, el("h3", t("profile.13f475c626")));
    if (!workspaceState.profiles.has(j.run_id))
        loadProfile(j);
    if (j.profile?.person_id) {
        const personButton = el("button", t("profile.576d0e3e0d"), "inspect-process");
        personButton.addEventListener("click", () => openPerson(j.profile.person_id));
        root.append(personButton);
        if (j.persona?.soul)
            root.append(el("h3", t("profile.3d4ec3a219", { p0: j.persona.revision })), readable(j.persona.soul, `persona-soul:${j.run_id}`, "soul-text"));
    }
    if (profile?.soul?.available) {
        root.append(readable(profile.soul.content, `soul:${j.run_id}`, "soul-text"));
        const source = profile.soul.source;
        root.append(el("p", source.kind === "frozen_source"
            ? `${source.path} @ ${short(source.commit, 12)}`
            : t("profile.70be7d71ef", { p0: source.version }), "process-meta"));
        root.append(el("code", t("profile.48eec06514", { p0: profile.soul.sha256 })));
    }
    else
        root.append(el("p", profile?.error ||
            describe(profile?.soul?.reason) ||
            t("profile.241dbcfe0e")));
    if (profile?.error || profile?.soul?.available === false) {
        const retry = el("button", t("profile.0a9f4350f4"), "mention");
        retry.addEventListener("click", () => {
            workspaceState.profiles.delete(j.run_id);
            workspaceState.detailKey = "";
            details(j);
        });
        root.append(retry);
    }
    root.append(el("p", t("profile.d92203c3f9"), "process-meta"));
    root.append(el("h3", t("profile.b6bb9f0247")));
    const dl = el("dl");
    for (const [k, v] of [
        [t("profile.6ec7d273ff"), name(j)],
        [t("profile.5253040db8"), j.id],
        [t("profile.c47b54e84e"), role(j)],
        [t("profile.c98e118e0a"), j.model],
        [t("profile.d43d31419b"), j.backend],
        [
            t("profile.6d93811810"),
            `${j.repository} · ${j.write ? t("profile.f1fbda2077") : t("profile.82a3f3881e")}`,
        ],
        [
            t("profile.7a6335b379"),
            j.persona?.memory
                ? t("profile.291bec195a")
                : j.profile?.use_memory
                    ? t("profile.73432d8ce5")
                    : t("profile.4547515a3c"),
        ],
        [
            t("profile.9db1176b4e"),
            j.profile?.required_tools?.join("、") || t("profile.7409a60806"),
        ],
        [
            t("profile.c57c08eaa5"),
            label("SessionEventKind", s?.events?.at(-1)?.title || "") ||
                t("profile.27a3021cc5"),
        ],
        [t("profile.bb4c173a76"), time(s?.modified_at)],
        [t("profile.18906f317e"), `${j.attempt} / ${j.profile?.max_attempts || 3}`],
        ["Session", s?.session_id || t("profile.e687470d7e")],
        [
            t("profile.2f191177ec"),
            j.dependencies.join("、") || t("profile.484d556139"),
        ],
    ])
        dl.append(el("dt", k), el("dd", v));
    root.append(dl);
    if (j.reason)
        root.append(el("h3", t("profile.97c7c7de85")), el("div", j.reason, "reason"));
    if (j.observation_error)
        root.append(el("p", t("profile.6ec3e45740")));
    const records = workspaceState.data.boards.flatMap((b) => b.retained_messages || []), sent = records.filter((m) => m.message.task_id === j.id), ids = j.context?.message_ids || [];
    root.append(el("h3", t("profile.2971ef6a3e")), el("p", t("profile.02b536a336", { p0: sent.length, p1: ids.length })));
    if (j.report?.summary) {
        root.append(el("h3", t("profile.1051b21266")), readable(j.report.summary, `report-summary:${j.run_id}`));
        const report = fold(`report:${j.run_id}`, t("profile.4685d41801"), "profile-section");
        for (const text of j.report.findings || [])
            if (text)
                report.append(el("p", t("profile.b8451eebe5") + text));
        for (const text of j.report.limitations || [])
            if (text)
                report.append(el("p", t("profile.4d7ab217a4") + text));
        root.append(report);
    }
    const evidence = fold(`evidence:${j.run_id}`, t("profile.e94661dc67"), "profile-section");
    for (const [k, v] of [
        [t("profile.b80b726024"), j.run_id],
        [t("profile.958e4091eb"), j.source_commit],
        ["SuperPOD", j.superpod_commit],
        ["Prompt SHA-256", j.prompt_digest],
        [t("profile.bf08c94e0f"), j.config_digest],
        [t("profile.47496076bd"), j.candidate_commit],
        [t("profile.a33bafc764"), j.persona?.memory?.sha256],
    ]) {
        if (!v)
            continue;
        const b = el("div", undefined, "binding");
        b.append(el("span", k), el("code", v));
        evidence.append(b);
    }
    ids.forEach((id) => evidence.append(el("p", id)));
    if (j.context?.digest)
        evidence.append(el("code", j.context.digest));
    root.append(evidence, el("p", t("profile.080c0efdd0"), "process-meta"));
    root.parentElement.scrollTop = scroll;
}
