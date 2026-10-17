/** Remove only a redundant speaker prefix from the presentation copy. */
export function contributionText(text: string, name: string): string {
  const content = text.trimStart();
  for (const prefix of [`${name}:`, `${name}：`, `@${name}:`, `@${name}：`]) {
    if (content.startsWith(prefix)) return content.slice(prefix.length).trimStart();
  }
  return text;
}
