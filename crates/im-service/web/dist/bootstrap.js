import { initWorkspace } from "./workspace.js";
import { initCollaboration } from "./collaboration.js";
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
import { initExperience } from "./experience.js";
import { busy } from "./feedback.js";
import { initIterations, refreshIterations } from "./iterations.js";
import { loadScenarioCatalog } from "./scenario-catalog.js";
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
    initIterations();
    initWorkspace();
    initCollaboration();
    initExperience();
    await loadScenarioCatalog();
    initNavigation();
    document.addEventListener("workspace-services", () => void loadScenarioCatalog().then(refreshIterations));
    await loadGroups();
    const directory = new EventSource("/api/crystal/events");
    let revision = -1;
    directory.addEventListener("status", (event) => {
        const data = JSON.parse(event.data).metrics;
        metrics(data);
        refreshEvolution();
        refreshIterations();
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
        refreshEvolution();
        refreshIterations();
        void loadGroups();
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
    const done = busy(get("login-form").querySelector("button"));
    if (!done)
        return;
    try {
        await login(value("login-token"));
    }
    catch (error) {
        notice(error, "login-status");
    }
    finally {
        done();
    }
};
try {
    let session = null;
    try {
        session = await api("/api/operator/session");
    }
    catch (error) {
        if (!location.hash.startsWith("#access="))
            throw error;
    }
    if (session && (session.local_operator === true || !location.hash.startsWith("#access="))) {
        if (location.hash.startsWith("#access="))
            history.replaceState(null, "", location.pathname + "#groups");
        await start();
        get("logout").hidden = session.local_operator === true;
    }
    else {
        const token = new URLSearchParams(location.hash.slice(1)).get("access") || "";
        history.replaceState(null, "", location.pathname + "#groups");
        await login(token);
    }
}
catch (error) {
    notice(error, "login-status");
}
