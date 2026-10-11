import type { Message } from "./types.js";
/** Only replies whose two endpoints are in the loaded history prove an exchange. */
export function replyEdges(
  messages: Message[],
): { from: string; to: string; sequence: number }[] {
  const loaded = new Map(
    messages.map((message) => [message.sequence, message]),
  );
  return messages.flatMap((message) => {
    const parent =
      message.reply_to === null ? undefined : loaded.get(message.reply_to);
    return parent &&
      parent.group_id === message.group_id &&
      parent.sender_id !== message.sender_id
      ? [
          {
            from: message.sender_id,
            to: parent.sender_id,
            sequence: message.sequence,
          },
        ]
      : [];
  });
}
