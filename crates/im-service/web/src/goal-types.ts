export interface Scenario {
  id: string;
  name: string;
  description: string;
  fields: { id: string; label: string; options: string[] }[];
}
export interface Work {
  id: string;
  person_id: string;
  instruction: string;
  state: "ready" | "running" | "submitted" | "failed";
  attempt: number;
  deadline_ms: number | null;
  result: { summary: string; succeeded: boolean; evidence: unknown } | null;
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
  state: "active" | "completed" | "cancelled";
  revision: number;
  work: Work[];
}
