export interface Draft {
  text: string;
  reply: number | null;
}
export interface PendingMessage {
  group_id: string;
  request_id: string;
  text: string;
  reply_to: number | null;
}

/** A request belongs to its conversation, including while another one is open. */
export class MessageDrafts {
  private drafts = new Map<string, Draft>();
  private pending = new Map<string, PendingMessage>();
  private sending = new Set<string>();
  private discussions = new Map<string, Record<string, unknown> | null>();

  save(id: string, draft: Draft): void { this.drafts.set(id, draft); }
  get(id: string): Draft { return this.drafts.get(id) || { text: "", reply: null }; }
  isSending(id: string): boolean { return this.sending.has(id); }

  begin(id: string, draft: Draft): PendingMessage | null {
    if (this.isSending(id) || !draft.text.trim()) return null;
    const text = draft.text.trim();
    let message = this.pending.get(id);
    if (!message || message.text !== text || message.reply_to !== draft.reply) {
      if (message) this.discussions.delete(message.request_id);
      message = { group_id: id, request_id: crypto.randomUUID(), text, reply_to: draft.reply };
      this.pending.set(id, message);
    }
    this.save(id, draft);
    this.sending.add(id);
    return message;
  }

  complete(message: PendingMessage): void {
    this.pending.delete(message.group_id);
    this.discussions.delete(message.request_id);
    const draft = this.get(message.group_id);
    if (draft.text.trim() === message.text && draft.reply === message.reply_to)
      this.drafts.delete(message.group_id);
  }

  finish(id: string): void { this.sending.delete(id); }
  discussion(message: PendingMessage): Record<string, unknown> | null | undefined {
    return this.discussions.get(message.request_id);
  }
  setDiscussion(message: PendingMessage, goal: Record<string, unknown> | null): void {
    this.discussions.set(message.request_id, goal);
  }
}
