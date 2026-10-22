import { initWorkspace } from "./workspace.js";
import { initCollaboration } from "./collaboration.js";
import { initTopics } from "./topics.js";
import { initEvolution, refreshEvolution } from "./evolution.js";
import { initServices } from "./settings.js";
import { apply } from "./i18n.js";
import { t } from "./i18n.js";
import { get, initDialogs, notice } from "./dom.js";
import { initDirectory, loadGroups, metrics } from "./directory.js";
import { initConversation, select } from "./conversation.js";
import { initGroups } from "./groups.js";
import { initMembers } from "./members.js";
import { initMessages } from "./messages.js";
import { initPeople } from "./people.js";
import { initGoals } from "./goal-form.js";
import { initNavigation } from "./navigation.js";
import { initExperience } from "./experience.js";
import { initIterations, refreshIterations } from "./iterations.js";
import { loadScenarioCatalog } from "./scenario-catalog.js";
apply();
let started = false;
async function start() {
    if (started)
        return;
    started = true;
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
    const directory = new EventSource("/api/v1/messaging/events");
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
            document.dispatchEvent(new Event("workspace-directory"));
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
}
try {
    if (location.hash.startsWith("#access="))
        history.replaceState(null, "", location.pathname + "#groups");
    await start();
}
catch (error) {
    notice(error);
}
