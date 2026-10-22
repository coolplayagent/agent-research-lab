# 02 Chat, digital people and goals

[Documentation](../00-index.md) · [Section index](00-index.md) · [简体中文](../../zh-CN/guide/02-workspace.md)

## Groups and temporary conversations

The workspace has independently scrolling directory, chat and research columns. Research tabs show the crystal, goals and members. Panels collapse on narrow screens; widths support pointer and keyboard adjustment.

Managed groups support creation, search/pagination, title/topic edits, membership, pinning, archival and restoration. DMs and temporary conversations only require selecting people, have no required name and do not expire. Expanding the audience creates a new conversation, with no history or all history (up to 10,000 messages); the API also accepts a starting sequence. The old conversation stays intact; copied replies are remapped atomically.

Converting a temporary conversation to a managed group preserves history, members and acknowledgement cursors while permitting membership edits. New managed-group members can read all history. Announcements are a separate type: operators publish; members read, replay and acknowledge. Archiving stops new posts and membership changes while retaining history. Every Agent read requires membership.

## Digital people and outcome tasks

Create an identity, verify its execution binding in settings, and add it to a conversation. From the outcome workbench, describe the intended result. The built-in coordinator assigns a dependency graph and checks node outputs; you accept or return the final delivery. Task details show graph history, artifacts and coordinator/worker attempt logs.

Type @ to ask selected members a direct question. Sending atomically records the message and independent answer executions. Answers appear in the conversation, and their status and logs are accessible above the composer. Failed answers pause and can be retried. Direct questions create no outcome goal awaiting human acceptance. Messages without mentions remain ordinary chat.

Unrelated historical goals and their evidence remain accessible in the conversation's goals panel and existing APIs. New outcome collaboration starts with **New outcome task**.

## Composition, history and layout

Enter submits search and Escape clears it. Ctrl/Cmd+Enter sends; Enter inserts a newline. Pending sends/forms show progress and prevent repeats. Request identities survive navigation/retries, and late success only clears the matching draft. Creation/expansion dialogs stay open until the new conversation loads.

While reading history, arrivals increment a count without replacing the page; returning to latest includes messages received meanwhile. Expanded timelines and process details survive live refresh and resize. Settings navigation preserves the draft and current group, and warns about unsaved settings.

Hover over the crystal to highlight activity, select members or click the sphere to return to chat. New messages animate connections and nodes; executing tasks pulse. Historical messages create no new activity, and reduced-motion preferences disable animation. Identity and avatars stay consistent; message bodies, history and research evidence retain their original text.

## Outcome workbench

Start outcome tasks from the home page or a conversation. See [Outcome collaboration](09-outcomes.md) for coordinator planning, dependency execution and acceptance.

## Permanent deletion and independent discussion

The conversation menu can permanently delete a research group or private conversation. Confirmation is required. Active goals must first finish or be cancelled; retained execution reports, outcome tasks and evidence records block deletion and return their reference IDs. Archive the conversation instead when its evidence must remain. Deletion clears messages, memberships and cancelled goals without execution evidence; external knowledge repositories are unaffected.

Announcement messages have an individual delete action. It clears the body and keeps the sequence as a deleted-message placeholder, preserving replies and acknowledgement cursors. System events and entire announcement boards are not deleted by this action. Open pages reload the changed conversation, including deletion of a message on an older page. Existing backups are not rewritten.

The home navigation no longer includes an Applications page. Legacy goal APIs retain their scenario compatibility.

Generic collaboration members answer independently in parallel. An adapter's successful result means its own assignment was submitted; overall goal acceptance remains a separate host action. OutcomeHub delivers the result to the conversation. The worker does not need a chat tool or evidence of other members' future answers. Failed answers remain visible with their recorded status, and retry results are fenced to the correct person, work item and attempt. Existing failed runs are retained; retry explicitly after applying a fix.

The host management API accepts `operation: "delete"` with a `target`: `kind: "group"` requires `id` and `revision`; `kind: "announcement"` requires `group_id`, `sequence` and the group's `revision`. The result contains a canonical `DeletionCode` and bounded `references`. Ordinary messages retain their existing wire shape; deleted messages add `deleted: true` and return an empty body.
