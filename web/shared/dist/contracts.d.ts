export declare const TaskKind: {
    readonly Outcome: "outcome";
    readonly Question: "question";
};
export type TaskKind = (typeof TaskKind)[keyof typeof TaskKind];
export declare const TaskState: {
    readonly Planning: "planning";
    readonly Running: "running";
    readonly WaitingInput: "waiting_input";
    readonly Paused: "paused";
    readonly Review: "review";
    readonly Completed: "completed";
    readonly Cancelled: "cancelled";
};
export type TaskState = (typeof TaskState)[keyof typeof TaskState];
export declare const TaskNodeState: {
    readonly Pending: "pending";
    readonly Running: "running";
    readonly Submitted: "submitted";
    readonly Approved: "approved";
    readonly Failed: "failed";
    readonly Stale: "stale";
};
export type TaskNodeState = (typeof TaskNodeState)[keyof typeof TaskNodeState];
export declare const TaskEventKind: {
    readonly Created: "created";
    readonly Claimed: "claimed";
    readonly Submitted: "submitted";
    readonly Checked: "checked";
    readonly Planned: "planned";
    readonly Revised: "revised";
    readonly Clarification: "clarification";
    readonly Supplemented: "supplemented";
    readonly Paused: "paused";
    readonly BudgetChanged: "budget_changed";
    readonly Delivered: "delivered";
    readonly Accepted: "accepted";
    readonly Returned: "returned";
    readonly Cancelled: "cancelled";
    readonly Expired: "expired";
};
export type TaskEventKind = (typeof TaskEventKind)[keyof typeof TaskEventKind];
export declare const TaskErrorCode: {
    readonly Invalid: "task_invalid";
    readonly Conflict: "task_conflict";
    readonly StaleAttempt: "task_stale_attempt";
    readonly Budget: "task_budget_exhausted";
    readonly Unavailable: "task_executor_unavailable";
    readonly NotFound: "task_not_found";
    readonly Forbidden: "task_forbidden";
    readonly Protocol: "task_protocol_invalid";
};
export type TaskErrorCode = (typeof TaskErrorCode)[keyof typeof TaskErrorCode];
export declare const DeletionCode: {
    readonly Deleted: "deleted";
    readonly NotFound: "not_found";
    readonly RevisionConflict: "revision_conflict";
    readonly ActiveGoals: "active_goals";
    readonly EvidenceReferences: "evidence_references";
    readonly Unsupported: "unsupported";
};
export type DeletionCode = (typeof DeletionCode)[keyof typeof DeletionCode];
export declare const CodexEventKind: {
    readonly ThreadStarted: "thread.started";
    readonly TurnStarted: "turn.started";
    readonly TurnCompleted: "turn.completed";
    readonly TurnFailed: "turn.failed";
    readonly ItemStarted: "item.started";
    readonly ItemUpdated: "item.updated";
    readonly ItemCompleted: "item.completed";
    readonly Error: "error";
};
export type CodexEventKind = (typeof CodexEventKind)[keyof typeof CodexEventKind];
export declare const ClaudeResultState: {
    readonly Success: "success";
    readonly ErrorDuringExecution: "error_during_execution";
    readonly ErrorMaxTurns: "error_max_turns";
    readonly ErrorMaxBudget: "error_max_budget_usd";
    readonly ErrorStructuredOutput: "error_max_structured_output_retries";
};
export type ClaudeResultState = (typeof ClaudeResultState)[keyof typeof ClaudeResultState];
export declare const PiEventKind: {
    readonly Response: "response";
    readonly AgentStart: "agent_start";
    readonly AgentEnd: "agent_end";
    readonly AgentSettled: "agent_settled";
    readonly TurnStart: "turn_start";
    readonly TurnEnd: "turn_end";
    readonly MessageStart: "message_start";
    readonly MessageUpdate: "message_update";
    readonly MessageEnd: "message_end";
    readonly ToolStart: "tool_execution_start";
    readonly ToolUpdate: "tool_execution_update";
    readonly ToolEnd: "tool_execution_end";
    readonly CompactionStart: "auto_compaction_start";
    readonly CompactionEnd: "auto_compaction_end";
    readonly RetryStart: "auto_retry_start";
    readonly RetryEnd: "auto_retry_end";
};
export type PiEventKind = (typeof PiEventKind)[keyof typeof PiEventKind];
export declare const PiStopReason: {
    readonly Stop: "stop";
    readonly Length: "length";
    readonly ToolUse: "toolUse";
    readonly Error: "error";
    readonly Aborted: "aborted";
};
export type PiStopReason = (typeof PiStopReason)[keyof typeof PiStopReason];
export declare const DshTurnReason: {
    readonly Completed: "completed";
    readonly MaxTokens: "max-tokens";
    readonly Aborted: "aborted";
    readonly Blocked: "blocked";
    readonly Error: "error";
    readonly Interrupted: "interrupted";
};
export type DshTurnReason = (typeof DshTurnReason)[keyof typeof DshTurnReason];
export declare const BindingStage: {
    readonly Discover: "discover";
    readonly Runtime: "runtime";
    readonly Configuration: "configuration";
    readonly Execute: "execute";
    readonly Persist: "persist";
};
export type BindingStage = (typeof BindingStage)[keyof typeof BindingStage];
export declare const AdapterTestStage: {
    readonly Discover: "discover";
    readonly Runtime: "runtime";
    readonly Configuration: "configuration";
    readonly Execute: "execute";
    readonly Persist: "persist";
};
export type AdapterTestStage = (typeof AdapterTestStage)[keyof typeof AdapterTestStage];
export declare const BindingOperationState: {
    readonly Running: "running";
    readonly Succeeded: "succeeded";
    readonly Failed: "failed";
    readonly Cancelled: "cancelled";
    readonly Interrupted: "interrupted";
};
export type BindingOperationState = (typeof BindingOperationState)[keyof typeof BindingOperationState];
export declare const AdapterTestOperationState: {
    readonly Running: "running";
    readonly Succeeded: "succeeded";
    readonly Failed: "failed";
    readonly Cancelled: "cancelled";
    readonly Interrupted: "interrupted";
};
export type AdapterTestOperationState = (typeof AdapterTestOperationState)[keyof typeof AdapterTestOperationState];
export declare const BindingStepState: {
    readonly Pending: "pending";
    readonly Running: "running";
    readonly Passed: "passed";
    readonly Failed: "failed";
    readonly Skipped: "skipped";
};
export type BindingStepState = (typeof BindingStepState)[keyof typeof BindingStepState];
export declare const AdapterTestStepState: {
    readonly Pending: "pending";
    readonly Running: "running";
    readonly Passed: "passed";
    readonly Failed: "failed";
    readonly Skipped: "skipped";
};
export type AdapterTestStepState = (typeof AdapterTestStepState)[keyof typeof AdapterTestStepState];
export declare const BindingErrorCode: {
    readonly ProgramMissing: "program_missing";
    readonly ProgramInvalid: "program_invalid";
    readonly LaunchFailed: "launch_failed";
    readonly ConfigurationInvalid: "configuration_invalid";
    readonly ServiceUnavailable: "service_unavailable";
    readonly IsolationUnavailable: "isolation_unavailable";
    readonly VerificationFailed: "verification_failed";
    readonly AuthenticationRequired: "authentication_required";
    readonly InvalidResponse: "invalid_response";
    readonly Timeout: "timeout";
    readonly Cancelled: "cancelled";
    readonly RevisionConflict: "revision_conflict";
    readonly PersistenceFailed: "persistence_failed";
    readonly Interrupted: "interrupted";
    readonly Busy: "busy";
    readonly ValidationRequired: "validation_required";
    readonly Superseded: "superseded";
};
export type BindingErrorCode = (typeof BindingErrorCode)[keyof typeof BindingErrorCode];
export declare const AdapterTestErrorCode: {
    readonly ProgramMissing: "program_missing";
    readonly ProgramInvalid: "program_invalid";
    readonly LaunchFailed: "launch_failed";
    readonly ConfigurationInvalid: "configuration_invalid";
    readonly ServiceUnavailable: "service_unavailable";
    readonly IsolationUnavailable: "isolation_unavailable";
    readonly VerificationFailed: "verification_failed";
    readonly AuthenticationRequired: "authentication_required";
    readonly InvalidResponse: "invalid_response";
    readonly Timeout: "timeout";
    readonly Cancelled: "cancelled";
    readonly RevisionConflict: "revision_conflict";
    readonly PersistenceFailed: "persistence_failed";
    readonly Interrupted: "interrupted";
    readonly Busy: "busy";
    readonly ValidationRequired: "validation_required";
    readonly Superseded: "superseded";
};
export type AdapterTestErrorCode = (typeof AdapterTestErrorCode)[keyof typeof AdapterTestErrorCode];
export declare const KnowledgeErrorCode: {
    readonly Failed: "knowledge_failed";
    readonly ExternalServiceRemoved: "knowledge_external_service_removed";
    readonly PathUnavailable: "knowledge_path_unavailable";
    readonly InvalidPath: "knowledge_invalid_path";
    readonly ExcludedPath: "knowledge_path_excluded";
    readonly DocumentLimit: "knowledge_document_limit";
    readonly SnapshotLimit: "knowledge_snapshot_limit";
    readonly FileLimit: "knowledge_file_limit";
    readonly DepthLimit: "knowledge_depth_limit";
    readonly Timeout: "knowledge_timeout";
    readonly ContentChanged: "knowledge_content_changed";
    readonly RevisionConflict: "knowledge_revision_conflict";
    readonly SymlinkRejected: "knowledge_symlink_rejected";
    readonly SpecialFileRejected: "knowledge_special_file_rejected";
    readonly IgnoreInvalid: "knowledge_ignore_invalid";
};
export type KnowledgeErrorCode = (typeof KnowledgeErrorCode)[keyof typeof KnowledgeErrorCode];
export declare const KnowledgeBackend: {
    readonly Git: "git";
    readonly Directory: "directory";
};
export type KnowledgeBackend = (typeof KnowledgeBackend)[keyof typeof KnowledgeBackend];
export declare const KnowledgeAccess: {
    readonly ReadOnly: "read_only";
    readonly ReadWrite: "read_write";
};
export type KnowledgeAccess = (typeof KnowledgeAccess)[keyof typeof KnowledgeAccess];
export declare const PullRequestState: {
    readonly Open: "OPEN";
    readonly Closed: "CLOSED";
    readonly Merged: "MERGED";
};
export type PullRequestState = (typeof PullRequestState)[keyof typeof PullRequestState];
export declare const ReviewState: {
    readonly Approved: "APPROVED";
    readonly ChangesRequested: "CHANGES_REQUESTED";
    readonly Dismissed: "DISMISSED";
    readonly Pending: "PENDING";
    readonly Commented: "COMMENTED";
    readonly Unknown: "UNKNOWN";
};
export type ReviewState = (typeof ReviewState)[keyof typeof ReviewState];
export declare const CheckState: {
    readonly Completed: "COMPLETED";
    readonly InProgress: "IN_PROGRESS";
    readonly Queued: "QUEUED";
    readonly Pending: "PENDING";
    readonly Waiting: "WAITING";
    readonly Requested: "REQUESTED";
};
export type CheckState = (typeof CheckState)[keyof typeof CheckState];
export declare const CheckConclusion: {
    readonly Success: "SUCCESS";
    readonly Failure: "FAILURE";
    readonly Error: "ERROR";
    readonly Pending: "PENDING";
    readonly Unknown: "UNKNOWN";
    readonly Neutral: "NEUTRAL";
    readonly Skipped: "SKIPPED";
    readonly Cancelled: "CANCELLED";
    readonly TimedOut: "TIMED_OUT";
    readonly ActionRequired: "ACTION_REQUIRED";
    readonly Stale: "STALE";
    readonly StartupFailure: "STARTUP_FAILURE";
};
export type CheckConclusion = (typeof CheckConclusion)[keyof typeof CheckConclusion];
export declare const MergeabilityState: {
    readonly Mergeable: "MERGEABLE";
    readonly Conflicting: "CONFLICTING";
    readonly Unknown: "UNKNOWN";
};
export type MergeabilityState = (typeof MergeabilityState)[keyof typeof MergeabilityState];
export declare const MergeQueueState: {
    readonly Clean: "CLEAN";
    readonly Blocked: "BLOCKED";
    readonly Behind: "BEHIND";
    readonly Dirty: "DIRTY";
    readonly Draft: "DRAFT";
    readonly HasHooks: "HAS_HOOKS";
    readonly Unknown: "UNKNOWN";
    readonly Unstable: "UNSTABLE";
};
export type MergeQueueState = (typeof MergeQueueState)[keyof typeof MergeQueueState];
export declare const VerificationVerdict: {
    readonly Pass: "pass";
    readonly Fail: "fail";
};
export type VerificationVerdict = (typeof VerificationVerdict)[keyof typeof VerificationVerdict];
export declare const QualitygateProfile: {
    readonly Quick: "quick";
    readonly Full: "full";
};
export type QualitygateProfile = (typeof QualitygateProfile)[keyof typeof QualitygateProfile];
export declare const QualitygateScope: {
    readonly Repository: "repository";
    readonly Delivery: "delivery";
    readonly Task: "task";
    readonly Path: "path";
};
export type QualitygateScope = (typeof QualitygateScope)[keyof typeof QualitygateScope];
export declare const QualitygateDecision: {
    readonly Pass: "pass";
    readonly Fail: "fail";
    readonly Incomplete: "incomplete";
};
export type QualitygateDecision = (typeof QualitygateDecision)[keyof typeof QualitygateDecision];
export declare const QualitygateSnapshotMode: {
    readonly Diff: "diff";
    readonly Staged: "staged";
    readonly Worktree: "worktree";
    readonly Mr: "mr";
};
export type QualitygateSnapshotMode = (typeof QualitygateSnapshotMode)[keyof typeof QualitygateSnapshotMode];
export declare const SandboxSessionState: {
    readonly Ready: "ready";
    readonly Stopped: "stopped";
};
export type SandboxSessionState = (typeof SandboxSessionState)[keyof typeof SandboxSessionState];
export declare const SandboxExecutionState: {
    readonly Completed: "completed";
    readonly Running: "running";
    readonly Failed: "failed";
};
export type SandboxExecutionState = (typeof SandboxExecutionState)[keyof typeof SandboxExecutionState];
export declare const SourceAccessStatus: {
    readonly DownloadedVerified: "downloaded-verified";
    readonly OnlineOnly: "online-only";
    readonly AccessGated: "access-gated";
    readonly NotFound: "not-found";
    readonly Candidate: "candidate";
};
export type SourceAccessStatus = (typeof SourceAccessStatus)[keyof typeof SourceAccessStatus];
export declare const FindingKind: {
    readonly Measured: "measured";
    readonly ExternalClaim: "external_claim";
    readonly Inference: "inference";
    readonly Unknown: "unknown";
};
export type FindingKind = (typeof FindingKind)[keyof typeof FindingKind];
export declare const DatasetPartition: {
    readonly Development: "development";
    readonly Validation: "validation";
    readonly Holdout: "holdout";
};
export type DatasetPartition = (typeof DatasetPartition)[keyof typeof DatasetPartition];
export declare const TrialVariant: {
    readonly Baseline: "baseline";
    readonly Candidate: "candidate";
};
export type TrialVariant = (typeof TrialVariant)[keyof typeof TrialVariant];
export declare const PromptEvaluationKind: {
    readonly Promotion: "promotion";
    readonly Novelty: "novelty";
};
export type PromptEvaluationKind = (typeof PromptEvaluationKind)[keyof typeof PromptEvaluationKind];
export declare const ContentIntegrityState: {
    readonly Complete: "complete";
    readonly Degraded: "degraded";
};
export type ContentIntegrityState = (typeof ContentIntegrityState)[keyof typeof ContentIntegrityState];
export declare const OutcomeKind: {
    readonly Finding: "finding";
    readonly Experiment: "experiment";
    readonly Counterexample: "counterexample";
    readonly Artifact: "artifact";
};
export type OutcomeKind = (typeof OutcomeKind)[keyof typeof OutcomeKind];
export declare const OutcomeReferenceKind: {
    readonly Uses: "uses";
    readonly Supports: "supports";
    readonly Challenges: "challenges";
};
export type OutcomeReferenceKind = (typeof OutcomeReferenceKind)[keyof typeof OutcomeReferenceKind];
export declare const OutcomePublicationState: {
    readonly Published: "published";
    readonly Withdrawn: "withdrawn";
};
export type OutcomePublicationState = (typeof OutcomePublicationState)[keyof typeof OutcomePublicationState];
export declare const OutcomeVerificationState: {
    readonly Unverified: "unverified";
    readonly Running: "running";
    readonly Passed: "passed";
    readonly Failed: "failed";
    readonly Unavailable: "unavailable";
    readonly Unknown: "unknown";
};
export type OutcomeVerificationState = (typeof OutcomeVerificationState)[keyof typeof OutcomeVerificationState];
export declare const VerificationRunState: {
    readonly Running: "running";
    readonly Passed: "passed";
    readonly Failed: "failed";
    readonly Unavailable: "unavailable";
    readonly Unknown: "unknown";
};
export type VerificationRunState = (typeof VerificationRunState)[keyof typeof VerificationRunState];
export declare const OutcomeAcceptanceState: {
    readonly Pending: "pending";
    readonly Accepted: "accepted";
    readonly Returned: "returned";
};
export type OutcomeAcceptanceState = (typeof OutcomeAcceptanceState)[keyof typeof OutcomeAcceptanceState];
export declare const KnowledgeWriteState: {
    readonly Pending: "pending";
    readonly Applied: "applied";
    readonly Failed: "failed";
    readonly Unknown: "unknown";
};
export type KnowledgeWriteState = (typeof KnowledgeWriteState)[keyof typeof KnowledgeWriteState];
export declare const OutcomeServiceKind: {
    readonly Verifier: "verifier";
    readonly Knowledge: "knowledge";
};
export type OutcomeServiceKind = (typeof OutcomeServiceKind)[keyof typeof OutcomeServiceKind];
export declare const OutcomeEventKind: {
    readonly Withdrawn: "withdrawn";
    readonly VerificationStarted: "verification_started";
    readonly VerificationFinished: "verification_finished";
    readonly Decision: "decision";
    readonly WriteStarted: "write_started";
    readonly WriteFinished: "write_finished";
    readonly WriteReconciled: "write_reconciled";
};
export type OutcomeEventKind = (typeof OutcomeEventKind)[keyof typeof OutcomeEventKind];
export declare const OutcomeErrorCode: {
    readonly ProtocolUpgradeRequired: "outcome_protocol_upgrade_required";
    readonly Invalid: "outcome_invalid";
    readonly Forbidden: "outcome_forbidden";
    readonly Conflict: "outcome_conflict";
    readonly StaleAttempt: "outcome_stale_attempt";
    readonly NotFound: "outcome_not_found";
    readonly Capacity: "outcome_capacity";
    readonly VerificationRequired: "outcome_verification_required";
    readonly BindingChanged: "outcome_binding_changed";
};
export type OutcomeErrorCode = (typeof OutcomeErrorCode)[keyof typeof OutcomeErrorCode];
export declare const ApiErrorCode: {
    readonly InvalidRequest: "invalid_request";
    readonly Backpressure: "backpressure";
    readonly StorageUnavailable: "storage_unavailable";
};
export type ApiErrorCode = (typeof ApiErrorCode)[keyof typeof ApiErrorCode];
export declare const InstallationPhase: {
    readonly Staged: "staged";
    readonly PreviousBackedUp: "previous_backed_up";
    readonly Activated: "activated";
    readonly Verified: "verified";
    readonly RolledBack: "rolled_back";
};
export type InstallationPhase = (typeof InstallationPhase)[keyof typeof InstallationPhase];
export declare const ScenarioCapability: {
    readonly Execute: "execute";
    readonly Evaluate: "evaluate";
    readonly Deliver: "deliver";
    readonly Activate: "activate";
    readonly Rollback: "rollback";
};
export type ScenarioCapability = (typeof ScenarioCapability)[keyof typeof ScenarioCapability];
export declare const CodingTaskRole: {
    readonly Research: "research";
    readonly Implement: "implement";
    readonly Review: "review";
};
export type CodingTaskRole = (typeof CodingTaskRole)[keyof typeof CodingTaskRole];
export declare const ServiceKind: {
    readonly Storage: "storage";
    readonly Executor: "executor";
    readonly Sandbox: "sandbox";
};
export type ServiceKind = (typeof ServiceKind)[keyof typeof ServiceKind];
export declare const ServiceManagement: {
    readonly External: "external";
    readonly LocalManaged: "local_managed";
};
export type ServiceManagement = (typeof ServiceManagement)[keyof typeof ServiceManagement];
export declare const ServiceConfigurationErrorCode: {
    readonly ManagementImmutable: "service_management_immutable";
    readonly KindImmutable: "service_kind_immutable";
    readonly CommandRequired: "adapter_command_required";
    readonly ValidationRequired: "adapter_validation_required";
    readonly Invalid: "service_configuration_invalid";
    readonly RuntimeUnavailable: "local_runtime_unavailable";
    readonly RevisionConflict: "service_revision_conflict";
};
export type ServiceConfigurationErrorCode = (typeof ServiceConfigurationErrorCode)[keyof typeof ServiceConfigurationErrorCode];
export declare const ServiceHealth: {
    readonly Healthy: "healthy";
    readonly Unavailable: "unavailable";
    readonly Disabled: "disabled";
    readonly ProtocolMismatch: "protocol_mismatch";
    readonly RestartRequired: "restart_required";
};
export type ServiceHealth = (typeof ServiceHealth)[keyof typeof ServiceHealth];
export declare const LocalRuntimeComponent: {
    readonly Storage: "storage";
    readonly Execution: "execution";
    readonly Knowledge: "knowledge";
};
export type LocalRuntimeComponent = (typeof LocalRuntimeComponent)[keyof typeof LocalRuntimeComponent];
export declare const LocalRuntimeState: {
    readonly Ready: "ready";
    readonly Standby: "standby";
    readonly Failed: "failed";
    readonly Missing: "missing";
    readonly Disabled: "disabled";
    readonly External: "external";
    readonly Development: "development";
    readonly Unsupported: "unsupported";
};
export type LocalRuntimeState = (typeof LocalRuntimeState)[keyof typeof LocalRuntimeState];
export declare const LocalRuntimeErrorCode: {
    readonly PackageMissing: "runtime_package_missing";
    readonly EndpointInUse: "runtime_endpoint_in_use";
    readonly StartupFailed: "runtime_startup_failed";
    readonly ReadinessTimeout: "runtime_readiness_timeout";
    readonly DataMissing: "runtime_data_missing";
    readonly RecoveryExhausted: "runtime_recovery_exhausted";
    readonly PlatformUnsupported: "runtime_platform_unsupported";
};
export type LocalRuntimeErrorCode = (typeof LocalRuntimeErrorCode)[keyof typeof LocalRuntimeErrorCode];
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
    readonly Cancelling: "cancelling";
    readonly Succeeded: "succeeded";
    readonly Failed: "failed";
    readonly Cancelled: "cancelled";
    readonly Unknown: "unknown";
};
export type WorkflowState = (typeof WorkflowState)[keyof typeof WorkflowState];
export declare const WorkflowNodeState: {
    readonly Pending: "pending";
    readonly TaskReady: "task_ready";
    readonly CheckingGate: "checking_gate";
    readonly CancelRequested: "cancel_requested";
    readonly Reconciling: "reconciling";
    readonly Waiting: "waiting";
    readonly Child: "child";
    readonly Succeeded: "succeeded";
    readonly Failed: "failed";
    readonly Cancelled: "cancelled";
    readonly Skipped: "skipped";
};
export type WorkflowNodeState = (typeof WorkflowNodeState)[keyof typeof WorkflowNodeState];
export declare const WorkflowFrameState: {
    readonly Active: "active";
    readonly Cancelling: "cancelling";
    readonly Succeeded: "succeeded";
    readonly Failed: "failed";
    readonly Cancelled: "cancelled";
};
export type WorkflowFrameState = (typeof WorkflowFrameState)[keyof typeof WorkflowFrameState];
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
export declare const BatchPromptSelection: {
    readonly Builtin: "builtin";
    readonly Registered: "registered";
};
export type BatchPromptSelection = (typeof BatchPromptSelection)[keyof typeof BatchPromptSelection];
export declare const BatchRegistrationState: {
    readonly Complete: "complete";
    readonly Incomplete: "incomplete";
    readonly Rejected: "rejected";
};
export type BatchRegistrationState = (typeof BatchRegistrationState)[keyof typeof BatchRegistrationState];
export declare const BatchEntryRegistrationState: {
    readonly Pending: "pending";
    readonly JobRecorded: "job_recorded";
    readonly WorkflowConfirmed: "workflow_confirmed";
    readonly AdvancedExistingConfirmed: "advanced_existing_confirmed";
    readonly Unknown: "unknown";
};
export type BatchEntryRegistrationState = (typeof BatchEntryRegistrationState)[keyof typeof BatchEntryRegistrationState];
export declare const BatchRegistrationPhase: {
    readonly Preflight: "preflight";
    readonly Collect: "collect";
    readonly Prepare: "prepare";
    readonly PersistPlan: "persist_plan";
    readonly RegisterJob: "register_job";
    readonly BindPersona: "bind_persona";
    readonly ConfirmWorkflow: "confirm_workflow";
    readonly PersistReceipt: "persist_receipt";
};
export type BatchRegistrationPhase = (typeof BatchRegistrationPhase)[keyof typeof BatchRegistrationPhase];
export declare const BatchRegistrationErrorCode: {
    readonly InvalidRequest: "invalid_request";
    readonly RequestConflict: "request_conflict";
    readonly FrozenBindingConflict: "frozen_binding_conflict";
    readonly FreshnessFailure: "freshness_failure";
    readonly FreshnessDrift: "freshness_drift";
    readonly OwnerProofUnavailable: "owner_proof_unavailable";
    readonly OwnerProofMismatch: "owner_proof_mismatch";
    readonly CurrentAttemptChanged: "current_attempt_changed";
    readonly DeadlineExceeded: "deadline_exceeded";
    readonly RegistrationFailure: "registration_failure";
    readonly ReceiptPersistenceFailure: "receipt_persistence_failure";
};
export type BatchRegistrationErrorCode = (typeof BatchRegistrationErrorCode)[keyof typeof BatchRegistrationErrorCode];
export declare const WorkflowLookupErrorCode: {
    readonly NotFound: "not_found";
};
export type WorkflowLookupErrorCode = (typeof WorkflowLookupErrorCode)[keyof typeof WorkflowLookupErrorCode];
export declare const WorkflowObservationErrorCode: {
    readonly Unavailable: "unavailable";
    readonly Mismatch: "mismatch";
};
export type WorkflowObservationErrorCode = (typeof WorkflowObservationErrorCode)[keyof typeof WorkflowObservationErrorCode];
export declare const NativeCaptureClass: {
    readonly Normal: "normal";
    readonly Cleanup: "cleanup";
};
export type NativeCaptureClass = (typeof NativeCaptureClass)[keyof typeof NativeCaptureClass];
export declare const NativeCaptureState: {
    readonly NotSpawned: "not_spawned";
    readonly Exited: "exited";
    readonly Signal: "signal";
    readonly Deadline: "deadline";
    readonly Overflow: "overflow";
    readonly Incomplete: "incomplete";
};
export type NativeCaptureState = (typeof NativeCaptureState)[keyof typeof NativeCaptureState];
export declare const NativeFilesystemOperation: {
    readonly OpenWrite: "open_write";
    readonly CreateDirectory: "create_directory";
    readonly Write: "write";
    readonly Truncate: "truncate";
    readonly Sync: "sync";
    readonly Chmod: "chmod";
    readonly Rename: "rename";
    readonly HardLink: "hard_link";
    readonly Unlink: "unlink";
    readonly RemoveDirectory: "remove_directory";
    readonly Lock: "lock";
    readonly Unlock: "unlock";
};
export type NativeFilesystemOperation = (typeof NativeFilesystemOperation)[keyof typeof NativeFilesystemOperation];
export declare const NativeFilesystemResult: {
    readonly NotAttempted: "not_attempted";
    readonly Succeeded: "succeeded";
    readonly Failed: "failed";
    readonly Unknown: "unknown";
};
export type NativeFilesystemResult = (typeof NativeFilesystemResult)[keyof typeof NativeFilesystemResult];
export declare const NativeFilesystemCompleteness: {
    readonly Complete: "complete";
    readonly Incomplete: "incomplete";
};
export type NativeFilesystemCompleteness = (typeof NativeFilesystemCompleteness)[keyof typeof NativeFilesystemCompleteness];
export declare const NativeFilesystemScope: {
    readonly Controller: "controller";
    readonly SelectedAttempt: "selected_attempt";
    readonly HistoricalRecovery: "historical_recovery";
    readonly CaptureTemporary: "capture_temporary";
};
export type NativeFilesystemScope = (typeof NativeFilesystemScope)[keyof typeof NativeFilesystemScope];
export declare const NativeFilesystemCategory: {
    readonly Json: "json";
    readonly Prompt: "prompt";
    readonly Protocol: "protocol";
    readonly Stream: "stream";
    readonly PrivateConfiguration: "private_configuration";
    readonly Cache: "cache";
    readonly Launcher: "launcher";
    readonly Directory: "directory";
};
export type NativeFilesystemCategory = (typeof NativeFilesystemCategory)[keyof typeof NativeFilesystemCategory];
export declare const CheckedAcquireFailureCode: {
    readonly AdmissionConflict: "admission_conflict";
    readonly Unknown: "owner_acquire_unknown";
};
export type CheckedAcquireFailureCode = (typeof CheckedAcquireFailureCode)[keyof typeof CheckedAcquireFailureCode];
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
export declare const ResearchInteraction: {
    readonly Discussion: "discussion";
    readonly Parallel: "parallel";
};
export type ResearchInteraction = (typeof ResearchInteraction)[keyof typeof ResearchInteraction];
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
