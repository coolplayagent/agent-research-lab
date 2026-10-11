import type { Person } from "./types.js";
export function mentionText(person: Person): string {
  return /[\s"@]/u.test(person.name) ? `@"${person.name.replaceAll('"', '')}"` : `@${person.name}`;
}
export function mentionNames(text: string): string[] {
  return [...text.matchAll(/(?:^|[\s(（])@(?:"([^"]+)"|([^\s@，。！？、:：;；)）]+))/gu)]
    .map((match) => match[1] || match[2]);
}
export function resolveMentions(text: string, members: Person[]): { people: Person[]; unresolved: string[] } {
  const people: Person[] = [], unresolved: string[] = [];
  for (const name of mentionNames(text)) {
    const matches = members.filter((person) => person.name === name || person.id === name);
    if (matches.length !== 1) { unresolved.push(name); continue; }
    if (!people.some((person) => person.id === matches[0].id)) people.push(matches[0]);
  }
  return { people, unresolved };
}
