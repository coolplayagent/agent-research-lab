import type {
  GoalState,
  WorkState,
  WorkResultCode,
  ScenarioCapability,
} from "/assets/shared/contracts.js";
export interface Scenario {
  id: string;
  protocol_version?: number;
  capabilities?: ScenarioCapability[];
  assignment_roles?: string[];
  execution_seconds?: number;
  public_process?: boolean;
  manual_review?: boolean;
  fields: { id: string; options: string[] }[];
  conversation?: { parameters: Record<string, unknown>; max_seconds: number };
}
export interface Work {
  id: string;
  person_id: string;
  instruction: string;
  state: WorkState;
  attempt: number;
  deadline_ms: number | null;
  result: {
    code?: WorkResultCode;
    summary: string;
    succeeded: boolean;
    evidence: unknown;
  } | null;
}
export interface Goal {
  iteration_id?: string | null;
  input: {
    id: string;
    group_id: string;
    title: string;
    objective: string;
    acceptance: string;
    scenario: string;
    max_seconds: number;
  };
  state: GoalState;
  revision: number;
  work: Work[];
}
