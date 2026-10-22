import type {
  TopicState,
  SubjectKind,
} from "/assets/shared/contracts.js";
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
}
