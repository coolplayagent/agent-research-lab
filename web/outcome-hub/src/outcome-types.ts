import type { OutcomeKind, OutcomeReferenceKind, OutcomePublicationState, OutcomeVerificationState, OutcomeAcceptanceState, VerificationRunState, KnowledgeWriteState, OutcomeServiceKind, OutcomeEventKind, ServiceManagement } from "/assets/shared/contracts.js";
export interface OutcomeEvidence { repository: string; commit: string; path: string; sha256: string }
export interface OutcomeReference { outcome_id: string; kind: OutcomeReferenceKind; reason: string }
export interface OutcomeInput {
  id: string; group_id: string; goal_id: string; kind: OutcomeKind; title: string; summary: string; conditions: string;
  evidence: OutcomeEvidence[]; references: OutcomeReference[]; supersedes: string | null;
  attempt: { goal_id: string; work_id: string; attempt: number; claim_id: string } | null;
  pins: { source: string | null; prompt: string | null; policy: string | null; superpod: string | null };
  knowledge: { service_id: string; repository: string; revision: string } | null;
}
export interface OutcomeSummary {
  id: string; group_id: string; goal_id: string; kind: OutcomeKind; title: string; summary: string;
  author_id: string; created_ms: number; publication: OutcomePublicationState;
  verification: OutcomeVerificationState; acceptance: OutcomeAcceptanceState; reference_count: number;
}
export interface OutcomeView {
  outcome: { input: OutcomeInput; author_id: string; created_ms: number; digest: string; sequence: number };
  publication: OutcomePublicationState; withdrawal_reason: string | null;
  verification: OutcomeVerificationState; acceptance: OutcomeAcceptanceState; reference_count: number;
  latest_verification: { input: { id: string; verifier_id: string }; deadline_ms: number;
    result: { state: VerificationRunState; summary: string; evidence: OutcomeEvidence[] } | null } | null;
  latest_decision: { id: string; state: OutcomeAcceptanceState; reason: string } | null;
  latest_write: { input: { id: string; repository: string; expected_revision: string }; deadline_ms: number;
    result: { state: KnowledgeWriteState; reference: string; revision: string; summary: string } | null } | null;
}
export interface OutcomePage { outcomes: OutcomeSummary[]; snapshot: number; next_after: number | null }
export interface OutcomeHistory { events: { sequence: number; kind: OutcomeEventKind; record: unknown; created_ms: number }[]; next_after: number | null }
export interface OutcomeService { management?: ServiceManagement; id: string; kind: OutcomeServiceKind; endpoint: string; enabled: boolean; protocol_version: number; outcome_kinds: OutcomeKind[]; dependencies: string[]; max_seconds: number }
