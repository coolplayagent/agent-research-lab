export function mentionText(person) {
    return /[\s"@]/u.test(person.name) ? `@"${person.name.replaceAll('"', '')}"` : `@${person.name}`;
}
export function mentionNames(text) {
    return [...text.matchAll(/(?:^|[\s(（])@(?:"([^"]+)"|([^\s@，。！？、:：;；)）]+))/gu)]
        .map((match) => match[1] || match[2]);
}
export function resolveMentions(text, members) {
    const people = [], unresolved = [];
    for (const name of mentionNames(text)) {
        const matches = members.filter((person) => person.name === name || person.id === name);
        if (matches.length !== 1) {
            unresolved.push(name);
            continue;
        }
        if (!people.some((person) => person.id === matches[0].id))
            people.push(matches[0]);
    }
    return { people, unresolved };
}
