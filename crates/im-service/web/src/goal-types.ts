import type {
  GoalState,
  WorkState,
  WorkResultCode,
} from "/assets/shared/contracts.js";
export interface Scenario {
  id: string;
  fields: { id: string; options: string[] }[];
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
