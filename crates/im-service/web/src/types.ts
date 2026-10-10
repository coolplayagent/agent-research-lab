export type Kind = "conversation" | "temporary" | "board";
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
  presence: string;
  connections: number;
}
export interface Member {
  person_id: string;
  presence: string;
  connections: number;
}
export interface Message {
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
