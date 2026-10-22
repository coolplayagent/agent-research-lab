//! Stable IM identities and actor-scoped group discovery, independent of execution apps.
use crate::{Hub, store::Store};
use anyhow::{Result, ensure};
use collaboration_model::*;
use rusqlite::params;
use serde_json::{Value, json};
impl Store {
    pub fn save_person(&mut self, person: &Person) -> Result<()> {
        person.validate()?;
        let tx = self.connection.transaction()?;
        tx.execute("INSERT INTO actors(id,name,application_id) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET name=excluded.name,application_id=excluded.application_id,archived=0", params![person.id, person.name, person.application_id])?;
        ensure!(
            tx.query_row("SELECT COUNT(*) FROM actors", [], |r| r.get::<_, usize>(0))?
                <= MAX_ACTORS,
            "actor capacity exhausted"
        );
        tx.commit()?;
        Ok(())
    }
    pub fn archive_unbound(
        &mut self,
        retained: &std::collections::BTreeSet<String>,
    ) -> Result<usize> {
        ensure!(
            retained.len() <= 20_000,
            "retained actor list exceeds bound"
        );
        for id in retained {
            collaboration_model::id(id)?;
        }
        let tx = self.connection.transaction()?;
        let ids = {
            let mut query =
                tx.prepare("SELECT id FROM actors WHERE archived=0 AND id!='operator'")?;
            query
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        let mut count = 0;
        for id in ids.into_iter().filter(|id| !retained.contains(id)) {
            tx.execute(
                "UPDATE actors SET archived=1,presence='offline' WHERE id=?1",
                [&id],
            )?;
            tx.execute("DELETE FROM grants WHERE actor_id=?1", [&id])?;
            count += 1;
        }
        tx.commit()?;
        Ok(count)
    }
}
impl Hub {
    pub fn people(&self, after: &str, query: &str, limit: usize) -> Result<Value> {
        ensure!(
            limit > 0 && limit <= 100 && query.len() <= 256,
            "directory page exceeds bound"
        );
        let catalog = self.shared.catalog.read().unwrap();
        let query = query.to_lowercase();
        let rows: Vec<_> = catalog
            .people
            .values()
            .filter(|p| {
                p.id != "operator"
                    && !catalog.archived_people.contains(&p.id)
                    && p.id.as_str() > after
                    && (p.name.to_lowercase().contains(&query)
                        || p.id.to_lowercase().contains(&query))
            })
            .take(limit + 1)
            .cloned()
            .collect();
        let next = (rows.len() > limit).then(|| rows[limit - 1].id.clone());
        let total = catalog.people.len()
            - catalog.archived_people.len()
            - usize::from(catalog.people.contains_key("operator"));
        drop(catalog);
        let people = rows.into_iter().take(limit).map(|p| {
            let status = self.presence(&p.id)?;
            Ok(json!({"id":p.id,"name":p.name,"application_id":p.application_id,"presence":status.presence,"connections":status.connections}))
        }).collect::<Result<Vec<_>>>()?;
        Ok(json!({"people":people,"next_after":next,"total":total}))
    }
    pub fn inbox(&self, token: &str, after: &str, limit: usize) -> Result<Value> {
        let actor = self.authenticate(token)?;
        ensure!(limit > 0 && limit <= 100, "inbox page exceeds bound");
        let catalog = self.shared.catalog.read().unwrap();
        let groups: Vec<_> = catalog
            .groups
            .values()
            .filter(|g| {
                g.id.as_str() > after
                    && catalog
                        .members
                        .get(&g.id)
                        .is_some_and(|m| m.contains(&actor))
            })
            .take(limit + 1)
            .collect();
        let next = (groups.len() > limit).then(|| groups[limit - 1].id.clone());
        Ok(
            json!({"person_id":actor,"groups":groups.into_iter().take(limit).collect::<Vec<_>>(),"next_after":next}),
        )
    }
    pub fn person(&self, actor: &str) -> Result<Person> {
        self.shared
            .catalog
            .read()
            .unwrap()
            .people
            .get(actor)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown digital person"))
    }
    pub async fn register_operator(&self) -> Result<()> {
        self.save_person(Person {
            id: "operator".into(),
            name: "Operator".into(),
            application_id: None,
        })
        .await?;
        self.set_presence("operator", Presence::Online).await
    }
}
#[cfg(test)]
#[path = "directory_tests.rs"]
mod tests;
