import type {
  TopicState,
  SubjectKind,
  EvolutionState,
  EvolutionOperation,
} from "/assets/shared/contracts.js";
import type { Goal } from "./goal-types.js";
export interface Topic {
  id: string;
  group_id: string;
  title: string;
  objective: string;
  state: TopicState;
  revision: number;
}
export interface Subject {
  id: string;
  topic_id: string;
  name: string;
  kind: SubjectKind;
  reference: string;
  revision: number;
}
export interface TopicRow {
  topic: Topic;
  subject_count: number;
  goal_count: number;
}
export interface SubjectRow {
  subject: Subject;
  node_count: number;
}
export interface Pins {
  source: string | null;
  prompt: string | null;
  policy: string | null;
  superpod: string | null;
}
export interface EvolutionInput {
  id: string;
  subject_id: string;
  parents: string[];
  version: string;
  reference: string;
  goal_id: string | null;
  pins: Pins;
}
export interface EvolutionRow {
  sequence: number;
  node: { input: EvolutionInput; generation: number; created_ms: number };
  subject: Subject;
  topic: Topic;
  status: EvolutionState;
  operation: EvolutionOperation;
  goal: Goal | null;
}
export interface GraphPage {
  nodes: EvolutionRow[];
  next_after: number | null;
}
