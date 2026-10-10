import type {
  GroupKind,
  Presence,
  GoalEventKind,
} from "/assets/shared/contracts.js";
export type Kind = GroupKind;
export interface Group {
  id: string;
  title: string;
  topic: string;
  kind: Kind;
  private: boolean;
  archived: boolean;
  pinned: boolean;
  revision: number;
  member_count: number;
  last_sequence: number;
  created_ms: number;
}
export interface Person {
  id: string;
  name: string;
  application_id: string | null;
  presence: Presence;
  connections: number;
}
export interface Member {
  person_id: string;
  presence: Presence;
  connections: number;
}
export interface Message {
  event?: {
    kind: GoalEventKind;
    goal_id: string;
    title: string;
    person_id: string | null;
    work_id: string | null;
    attempt: number | null;
  };
  sequence: number;
  group_id: string;
  sender_id: string;
  request_id: string;
  text: string;
  reply_to: number | null;
  accepted_ms: number;
}
export interface Page<T> {
  next_after: string | null;
  total: number;
  people: T[];
}
export interface GroupView {
  group: Group;
  members: { members: Member[]; next_after: string | null };
  messages: Message[];
}
export interface Directory {
  directory: { groups: Group[]; next_after: string | null };
  metrics: Record<string, unknown>;
}
