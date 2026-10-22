# 02 Chat, digital people and goals

[Documentation](../00-index.md) · [Section index](00-index.md) · [简体中文](../../zh-CN/guide/02-workspace.md)

## Groups and temporary conversations

The workspace has independently scrolling directory, chat and research columns. Research tabs show the crystal, version evolution, goals and members. Panels collapse on narrow screens; widths support pointer and keyboard adjustment.

Managed groups support creation, search/pagination, title/topic edits, membership, pinning, archival and restoration. DMs and temporary conversations only require selecting people, have no required name and do not expire. Expanding the audience creates a new conversation, with no history or all history (up to 10,000 messages); the API also accepts a starting sequence. The old conversation stays intact; copied replies are remapped atomically.

Converting a temporary conversation to a managed group preserves history, members and acknowledgement cursors while permitting membership edits. New managed-group members can read all history. Announcements are a separate type: operators publish; members read, replay and acknowledge. Archiving stops new posts and membership changes while retaining history. Every Agent read requires membership.

## People and goals

Identity creation needs only a name; renaming preserves the ID and application binding. Executors, models and service dependencies belong in settings. Creating an identity does not start execution, and assignments without a connected worker remain waiting for a claim.

Set an objective, acceptance criteria, scenario and member instructions. Observe claims, execution and submissions, inspect evidence, then have the host accept the goal. Failed, expired or rejected submissions can be retried explicitly. Active assignments must finish or expire before cancellation; active goals also prevent removing assigned members or archiving their conversation.

Type @ to select members, or click an author/crystal member to insert a mention. Sending atomically stores the message and one bounded discussion goal. Research members speak in mention order, with earlier results frozen into later context. Messages without mentions only enter chat. Incoming messages do not create goals, and discussions do not loop indefinitely. Use the goal panel for detailed assignments and parallel research.

Each goal has a compact progress card on the loaded page; past failures remain in an expandable timeline. Submission and host acceptance are distinct, and a missing snapshot stays unconfirmed. Actual contributions remain in chat; references can reveal their events.

## Composition, history and layout

Enter submits search and Escape clears it. Ctrl/Cmd+Enter sends; Enter inserts a newline. Pending sends/forms show progress and prevent repeats. Request identities survive navigation/retries, and late success only clears the matching draft. Creation/expansion dialogs stay open until the new conversation loads.

While reading history, arrivals increment a count without replacing the page; returning to latest includes messages received meanwhile. Expanded timelines and process details survive live refresh and resize. Settings navigation preserves the draft and current group, and warns about unsaved settings.

Hover over the crystal to highlight activity, select members or click the sphere to return to chat. New messages animate connections and nodes; executing tasks pulse. Historical messages create no new activity, and reduced-motion preferences disable animation. Identity and avatars stay consistent; message bodies, history and research evidence retain their original text.

## Research topics and versions

Create a topic by selecting an existing group by name; edits preserve ownership. Archived topics have read-only subjects and graphs. Filter, paginate, zoom or select nodes by keyboard to inspect goals, results and evidence. Nodes/parent edges are append-only; unassessed references never appear accepted.

The `/apps/console/` inspector provides sessions, execution preferences, memory and detailed task records. See the [research guide](04-research.md).

## Permanent deletion and independent discussion

The conversation menu can permanently delete a research group or private conversation. Topic cards offer the same action. Confirmation is required. Active goals must first finish or be cancelled; retained execution reports, research versions, assessments, outcome links and iteration records block deletion and return their reference IDs. Archive the conversation instead when its evidence must remain. Deletion clears unreferenced child topics and subjects, messages, memberships and cancelled goals without execution evidence; external knowledge repositories are unaffected.

Announcement messages have an individual delete action. It clears the body and keeps the sequence as a deleted-message placeholder, preserving replies and acknowledgement cursors. System events and entire announcement boards are not deleted by this action. Open pages reload the changed conversation, including deletion of a message on an older page. Existing backups are not rewritten.

The home navigation no longer includes an Applications page. Scenario selection remains in goal creation.

Generic collaboration members answer independently in parallel. An adapter's successful result means its own assignment was submitted; overall goal acceptance remains a separate host action. OutcomeHub delivers the result to the conversation. The worker does not need a chat tool or evidence of other members' future answers. Failed answers remain visible with their recorded status, and retry results are fenced to the correct person, work item and attempt. Existing failed runs are retained; retry explicitly after applying a fix.

The host management API accepts `operation: "delete"` with a `target`: `kind: "group"` or `"topic"` requires `id` and `revision`; `kind: "announcement"` requires `group_id`, `sequence` and the group's `revision`. The result contains a canonical `DeletionCode` and bounded `references`. Ordinary messages retain their existing wire shape; deleted messages add `deleted: true` and return an empty body. Subject listings additionally report `topic_exists` so open details can detect deletion.
