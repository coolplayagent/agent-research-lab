import { uuid } from "/assets/shared/identity.js";
/** A request belongs to its conversation, including while another one is open. */
export class MessageDrafts {
    drafts = new Map();
    pending = new Map();
    sending = new Set();
    discussions = new Map();
    save(id, draft) { this.drafts.set(id, draft); }
    get(id) { return this.drafts.get(id) || { text: "", reply: null }; }
    isSending(id) { return this.sending.has(id); }
    begin(id, draft) {
        if (this.isSending(id) || !draft.text.trim())
            return null;
        const text = draft.text.trim();
        let message = this.pending.get(id);
        if (!message || message.text !== text || message.reply_to !== draft.reply) {
            if (message)
                this.discussions.delete(message.request_id);
            message = { group_id: id, request_id: uuid(), text, reply_to: draft.reply };
            this.pending.set(id, message);
        }
        this.save(id, draft);
        this.sending.add(id);
        return message;
    }
    complete(message) {
        this.pending.delete(message.group_id);
        this.discussions.delete(message.request_id);
        const draft = this.get(message.group_id);
        if (draft.text.trim() === message.text && draft.reply === message.reply_to)
            this.drafts.delete(message.group_id);
    }
    finish(id) { this.sending.delete(id); }
    discussion(message) {
        return this.discussions.get(message.request_id);
    }
    setDiscussion(message, goal) {
        this.discussions.set(message.request_id, goal);
    }
}
