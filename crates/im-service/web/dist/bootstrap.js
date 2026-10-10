import { initTopics } from "./topics.js";
import { initEvolution, refreshEvolution } from "./evolution.js";
import { initServices } from "./settings.js";
import { apply } from "./i18n.js";
import { t } from "./i18n.js";
import { api } from "./api.js";
import { get, initDialogs, notice, value } from "./dom.js";
import { initDirectory, loadGroups, metrics } from "./directory.js";
import { initConversation, select } from "./conversation.js";
import { initGroups } from "./groups.js";
import { initMembers } from "./members.js";
import { initMessages } from "./messages.js";
import { initPeople } from "./people.js";
import { initGoals } from "./goal-form.js";
import { initNavigation } from "./navigation.js";
apply();
let started = false;
async function start() {
    if (started)
        return;
    started = true;
    get("login").hidden = true;
    get("workspace").hidden = false;
    initDialogs();
    initDirectory((group) => void select(group));
    initConversation();
    initGroups();
    initMembers();
    initMessages();
    initPeople();
    initGoals();
    initServices();
    initTopics();
    initEvolution();
    initNavigation();
    await loadGroups();
    const directory = new EventSource("/api/crystal/events");
    let revision = -1;
    directory.addEventListener("status", (event) => {
        const data = JSON.parse(event.data).metrics;
        metrics(data);
        const changed = Number(data.catalog_revision);
        if (revision !== changed) {
            revision = changed;
            refreshEvolution();
            void loadGroups();
        }
    });
    directory.onerror = () => {
        get("connection").textContent = t("bootstrap.dd2b0733b6");
    };
    directory.onopen = () => {
        get("connection").textContent = t("bootstrap.021bd7f674");
    };
    get("logout").onclick = async () => {
        try {
            await api("/api/operator/logout", {}, "operator-login");
            location.reload();
        }
        catch (error) {
            notice(error);
        }
    };
}
async function login(token) {
    await api("/api/operator/session", { token }, "operator-login");
    get("login-token").value = "";
    await start();
}
get("login-form").onsubmit = async (event) => {
    event.preventDefault();
    try {
        await login(value("login-token"));
    }
    catch (error) {
        notice(error, "login-status");
    }
};
try {
    if (location.hash.startsWith("#access=")) {
        const token = new URLSearchParams(location.hash.slice(1)).get("access") || "";
        history.replaceState(null, "", location.pathname + "#groups");
        await login(token);
    }
    else {
        await api("/api/operator/session");
        await start();
    }
}
catch (error) {
    notice(error, "login-status");
}
