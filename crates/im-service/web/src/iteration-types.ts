import type {
  ActivationState,
  CandidateState,
  EvaluationState,
  IterationState,
  RoundState,
} from "/assets/shared/contracts.js";

export interface IterationInput {
  id: string;
  group_id: string;
  subject_id: string;
  scenario_id: string;
  title: string;
  objective: string;
  acceptance: string;
  assignments: { person_id: string; instruction: string }[];
  baseline_version_id: string;
  parameters: Record<string, unknown>;
  max_rounds: number;
  max_seconds: number;
  total_budget_seconds: number;
}

export interface IterationRound {
  number: number;
  state: RoundState;
  goal_id: string;
  goal_attempts: Record<string, number>;
  baseline_version_id: string;
  started_ms: number;
  finished_ms: number | null;
  feedback: string;
  candidate: {
    version_id: string;
    producer_work_id: string;
    state: CandidateState;
  } | null;
  evaluation: {
    evaluator_work_id: string;
    goal_revision: number;
    state: EvaluationState;
    summary: string;
    evidence: unknown;
  } | null;
  activation: {
    state: ActivationState;
    reference: string;
    evidence: unknown;
  } | null;
}

export interface IterationRun {
  input: IterationInput;
  state: IterationState;
  revision: number;
  created_ms: number;
  updated_ms: number;
  used_seconds: number;
  baseline_version_id: string;
  rounds: IterationRound[];
}

export interface IterationPage {
  runs: IterationRun[];
  next_after: string | null;
}
