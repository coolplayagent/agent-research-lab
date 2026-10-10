export declare const ServiceKind: {
    readonly Storage: "storage";
    readonly Executor: "executor";
    readonly Sandbox: "sandbox";
};
export type ServiceKind = (typeof ServiceKind)[keyof typeof ServiceKind];
export declare const ServiceHealth: {
    readonly Healthy: "healthy";
    readonly Unavailable: "unavailable";
    readonly Disabled: "disabled";
    readonly ProtocolMismatch: "protocol_mismatch";
    readonly RestartRequired: "restart_required";
};
export type ServiceHealth = (typeof ServiceHealth)[keyof typeof ServiceHealth];
export declare const CodingAgent: {
    readonly ClaudeCode: "cc";
    readonly Codex: "codex";
    readonly DeepseekHarness: "hds";
    readonly Pi: "pi";
    readonly Custom: "custom";
};
export type CodingAgent = (typeof CodingAgent)[keyof typeof CodingAgent];
export declare const AgentProtocol: {
    readonly JsonStdioV1: "json_stdio_v1";
};
export type AgentProtocol = (typeof AgentProtocol)[keyof typeof AgentProtocol];
export declare const OriginStateKind: {
    readonly Active: "active";
    readonly Completed: "completed";
    readonly Revoked: "revoked";
};
export type OriginStateKind = (typeof OriginStateKind)[keyof typeof OriginStateKind];
export declare const Presence: {
    readonly Offline: "offline";
    readonly Online: "online";
    readonly Chatting: "chatting";
    readonly Busy: "busy";
};
export type Presence = (typeof Presence)[keyof typeof Presence];
export declare const GroupKind: {
    readonly Conversation: "conversation";
    readonly Temporary: "temporary";
    readonly Board: "board";
};
export type GroupKind = (typeof GroupKind)[keyof typeof GroupKind];
export declare const PersonKind: {
    readonly Fixed: "fixed";
    readonly Temporary: "temporary";
    readonly Research: "research";
};
export type PersonKind = (typeof PersonKind)[keyof typeof PersonKind];
export declare const GoalState: {
    readonly Active: "active";
    readonly Completed: "completed";
    readonly Cancelled: "cancelled";
};
export type GoalState = (typeof GoalState)[keyof typeof GoalState];
export declare const WorkState: {
    readonly Ready: "ready";
    readonly Running: "running";
    readonly Submitted: "submitted";
    readonly Failed: "failed";
};
export type WorkState = (typeof WorkState)[keyof typeof WorkState];
export declare const ExecutionState: {
    readonly Ready: "ready";
    readonly Running: "running";
    readonly Succeeded: "succeeded";
    readonly Failed: "failed";
    readonly Blocked: "blocked";
    readonly Unknown: "unknown";
    readonly Paused: "paused";
    readonly RetryWait: "retry_wait";
    readonly NeedsReconciliation: "needs_reconciliation";
    readonly WaitingDependencies: "waiting_dependencies";
    readonly WaitingTarget: "waiting_target";
    readonly Historical: "historical";
};
export type ExecutionState = (typeof ExecutionState)[keyof typeof ExecutionState];
export declare const WorkflowState: {
    readonly Running: "running";
    readonly Succeeded: "succeeded";
    readonly Failed: "failed";
    readonly Cancelled: "cancelled";
    readonly Unknown: "unknown";
};
export type WorkflowState = (typeof WorkflowState)[keyof typeof WorkflowState];
export declare const WorkflowNodeState: {
    readonly TaskReady: "task_ready";
};
export type WorkflowNodeState = (typeof WorkflowNodeState)[keyof typeof WorkflowNodeState];
export declare const EffectState: {
    readonly Applied: "applied";
    readonly Unknown: "unknown";
};
export type EffectState = (typeof EffectState)[keyof typeof EffectState];
export declare const ControllerPhase: {
    readonly Unknown: "unknown";
    readonly Idle: "idle";
    readonly Running: "running";
    readonly Error: "error";
    readonly Stopped: "stopped";
    readonly Starting: "starting";
    readonly Recovering: "recovering";
    readonly Refreshing: "refreshing";
};
export type ControllerPhase = (typeof ControllerPhase)[keyof typeof ControllerPhase];
export declare const MemoryWriteback: {
    readonly Pending: "pending";
    readonly Running: "running";
    readonly Done: "done";
    readonly Unknown: "unknown";
    readonly NotEnabled: "not_enabled";
};
export type MemoryWriteback = (typeof MemoryWriteback)[keyof typeof MemoryWriteback];
export declare const FollowupAdmission: {
    readonly Pending: "pending";
    readonly Running: "running";
    readonly Done: "done";
    readonly Blocked: "blocked";
};
export type FollowupAdmission = (typeof FollowupAdmission)[keyof typeof FollowupAdmission];
export declare const FollowupState: {
    readonly DeferredTaskScope: "deferred_task_scope";
    readonly DeferredBudget: "deferred_budget";
    readonly FollowupBlocked: "followup_blocked";
};
export type FollowupState = (typeof FollowupState)[keyof typeof FollowupState];
export declare const QueueState: {
    readonly Pending: "pending";
    readonly Running: "running";
    readonly Complete: "complete";
    readonly Waiting: "waiting";
    readonly NeedsReconciliation: "needs_reconciliation";
};
export type QueueState = (typeof QueueState)[keyof typeof QueueState];
export declare const DeliveryState: {
    readonly Delivered: "delivered";
    readonly Expired: "expired";
    readonly Pending: "pending";
};
export type DeliveryState = (typeof DeliveryState)[keyof typeof DeliveryState];
export declare const ExperimentTaskState: {
    readonly NotEnqueued: "not_enqueued";
    readonly InvalidBinding: "invalid_binding";
    readonly Succeeded: "succeeded";
    readonly InvalidReport: "invalid_report";
};
export type ExperimentTaskState = (typeof ExperimentTaskState)[keyof typeof ExperimentTaskState];
export declare const ProposalState: {
    readonly PendingHostReview: "pending_host_review";
};
export type ProposalState = (typeof ProposalState)[keyof typeof ProposalState];
export declare const GoalEventKind: {
    readonly Created: "goal_created";
    readonly Accepted: "goal_accepted";
    readonly Cancelled: "goal_cancelled";
    readonly Retried: "work_retried";
    readonly Claimed: "work_claimed";
    readonly Submitted: "work_submitted";
    readonly Failed: "work_failed";
};
export type GoalEventKind = (typeof GoalEventKind)[keyof typeof GoalEventKind];
export declare const ScenarioStatus: {
    readonly Ready: "scenario_ready";
    readonly Unavailable: "scenario_unavailable";
    readonly BatchComplete: "scenario_batch_complete";
};
export type ScenarioStatus = (typeof ScenarioStatus)[keyof typeof ScenarioStatus];
export declare const ApiNotice: {
    readonly BoundPolicyUnavailable: "bound_policy_unavailable";
    readonly BoundPolicyScope: "bound_policy_scope";
    readonly LineageUnavailable: "lineage_unavailable";
    readonly SnapshotReadFailed: "snapshot_read_failed";
    readonly ObserverOverflow: "observer_overflow";
    readonly ObserverInterrupted: "observer_interrupted";
    readonly PublicEventScope: "public_event_scope";
    readonly WorkflowObservationIncomplete: "workflow_observation_incomplete";
    readonly BoardUnavailable: "board_unavailable";
    readonly SnapshotLoading: "snapshot_loading";
    readonly ObserverUnavailable: "observer_unavailable";
    readonly MemoryScope: "memory_scope";
    readonly DirectoryLimit: "directory_limit";
    readonly JobLimit: "job_limit";
    readonly JobUnavailable: "job_unavailable";
    readonly BoardLimit: "board_limit";
    readonly GroupUnavailable: "group_unavailable";
};
export type ApiNotice = (typeof ApiNotice)[keyof typeof ApiNotice];
export declare const SessionItemStatus: {
    readonly InProgress: "in_progress";
    readonly Completed: "completed";
    readonly Failed: "failed";
    readonly Error: "error";
    readonly Unknown: "unknown";
};
export type SessionItemStatus = (typeof SessionItemStatus)[keyof typeof SessionItemStatus];
export declare const SessionEventKind: {
    readonly SessionStarted: "session_started";
    readonly TurnStarted: "turn_started";
    readonly TurnCompleted: "turn_completed";
    readonly ExecutionError: "execution_error";
    readonly ToolStarted: "tool_started";
    readonly ToolCompleted: "tool_completed";
    readonly AgentMessage: "agent_message";
    readonly FileChange: "file_change";
    readonly Activity: "activity";
};
export type SessionEventKind = (typeof SessionEventKind)[keyof typeof SessionEventKind];
export declare const WorkResultCode: {
    readonly ExecutorFailed: "executor_failed";
    readonly Completed: "research_completed";
    readonly Failed: "research_failed";
    readonly AdmissionFailed: "research_admission_failed";
};
export type WorkResultCode = (typeof WorkResultCode)[keyof typeof WorkResultCode];
