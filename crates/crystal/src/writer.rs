use super::*;
use std::time::Duration;

pub(crate) fn run(mut store: Store, mut rx: mpsc::Receiver<Command>, shared: Arc<Shared>) {
    let mut pending = None;
    while let Some(command) = pending.take().or_else(|| rx.blocking_recv()) {
        shared.metrics.queued.fetch_sub(1, Ordering::Relaxed);
        if shared.failed.load(Ordering::Acquire) {
            let error = Err(anyhow!(
                "durable writer fault; restart after storage recovery"
            ));
            match command {
                Command::Publish { reply, .. } => {
                    let _ = reply.send(error);
                }
                Command::Control { reply, .. } => {
                    let _ = reply.send(Err(anyhow!("durable writer fault")));
                }
            }
            continue;
        }
        match command {
            Command::Publish { .. } => {
                let mut batch = vec![command];
                // Bounded group commit amortizes fsync without acknowledging buffered writes.
                std::thread::sleep(Duration::from_micros(300));
                while batch.len() < 128 {
                    match rx.try_recv() {
                        Ok(command @ Command::Publish { .. }) => {
                            shared.metrics.queued.fetch_sub(1, Ordering::Relaxed);
                            batch.push(command);
                        }
                        Ok(command) => {
                            pending = Some(command);
                            break;
                        }
                        Err(_) => break,
                    }
                }
                publish_batch(&mut store, &shared, batch);
            }
            Command::Control { op, reply } => {
                let result = control(&mut store, &shared, op);
                if result.as_ref().err().is_some_and(storage_fault) {
                    shared.metrics.failures.fetch_add(1, Ordering::Relaxed);
                    shared.fail();
                    if let Ok(catalog) = store.catalog() {
                        *shared.catalog.write().unwrap() = catalog;
                    }
                }
                let _ = reply.send(result);
            }
        }
    }
}
fn publish_batch(store: &mut Store, shared: &Shared, batch: Vec<Command>) {
    let start = Instant::now();
    let result = (|| -> Result<Vec<Result<Receipt>>> {
        let mut remaining = MAX_MESSAGES.saturating_sub(shared.catalog.read().unwrap().messages);
        let tx = store.connection.transaction()?;
        let mut results = Vec::with_capacity(batch.len());
        for command in &batch {
            let Command::Publish {
                actor,
                input,
                credential,
                ..
            } = command
            else {
                unreachable!()
            };
            let result = (|| -> Result<Receipt> {
                if let Some(hash) = credential {
                    ensure!(
                        shared
                            .catalog
                            .read()
                            .unwrap()
                            .grants
                            .get(hash)
                            .is_some_and(|(id, expires)| id == actor && *expires > now_ms()),
                        "communication credential revoked or expired before commit"
                    );
                }
                store::append(&tx, actor, input, &mut remaining, credential.is_none())
            })();
            if result.as_ref().err().is_some_and(storage_fault) {
                return Err(result.unwrap_err());
            }
            results.push(result);
        }
        tx.commit()?;
        Ok(results)
    })();
    Metrics::sample(
        &shared.metrics.commit_ms,
        start.elapsed().as_secs_f64() * 1000.0,
    );
    let results = match result {
        Ok(results) => results,
        Err(error) => {
            shared.metrics.failures.fetch_add(1, Ordering::Relaxed);
            // Fail closed after an uncertain commit; recovery requires reopening the store.
            shared.fail();
            // Reload for observation only. Never invent ACKs.
            if let Ok(catalog) = store.catalog() {
                *shared.catalog.write().unwrap() = catalog;
            }
            shared.signal();
            batch
                .iter()
                .map(|_| Err(anyhow!("durable commit failed: {error}")))
                .collect()
        }
    };
    let mut publish = Vec::new();
    {
        let mut catalog = shared.catalog.write().unwrap();
        for receipt in results.iter().filter_map(|r| r.as_ref().ok()) {
            if !receipt.duplicate {
                catalog.messages += 1;
                if let Some(group) = catalog.groups.get_mut(&receipt.message.group_id) {
                    group.last_sequence = receipt.message.sequence;
                }
                publish.push(Arc::new(receipt.message.clone()));
            }
        }
    }
    for (command, result) in batch.into_iter().zip(results) {
        let Command::Publish { started, reply, .. } = command else {
            unreachable!()
        };
        match &result {
            Ok(receipt) => {
                if receipt.duplicate {
                    shared.metrics.duplicates.fetch_add(1, Ordering::Relaxed);
                } else {
                    shared.metrics.accepted.fetch_add(1, Ordering::Relaxed);
                }
                Metrics::sample(
                    &shared.metrics.ack_ms,
                    started.elapsed().as_secs_f64() * 1000.0,
                );
            }
            Err(_) => {
                shared.metrics.rejected.fetch_add(1, Ordering::Relaxed);
            }
        }
        let _ = reply.send(result);
    }
    for message in publish {
        // One shared message/ring per group; no per-recipient durable message copies.
        if let Some(feed) = shared.feeds.lock().unwrap().get(&message.group_id) {
            let _ = feed.send(message);
        }
    }
    shared.signal();
}
fn control(store: &mut Store, shared: &Shared, op: Control) -> Result<Value> {
    let result = match op {
        Control::Research(command) => {
            let write = matches!(&command, crate::research::ResearchCommand::Change(_));
            let result = store.research_command(command)?;
            if !write {
                return Ok(result);
            }
            result
        }
        Control::Goal(command) => {
            let read = matches!(&command, crate::goals::GoalCommand::List { .. });
            let result = store.goal_command(command)?;
            if read {
                return Ok(result);
            }
            let group = result["input"]["group_id"]
                .as_str()
                .context("goal group missing")?;
            let after = shared
                .catalog
                .read()
                .unwrap()
                .groups
                .get(group)
                .map_or(0, |g| g.last_sequence);
            let messages = store.history(group, after, 2)?;
            *shared.catalog.write().unwrap() = store.catalog()?;
            for message in messages {
                if let Some(feed) = shared.feeds.lock().unwrap().get(group) {
                    let _ = feed.send(Arc::new(message));
                }
            }
            result
        }
        #[cfg(test)]
        Control::ReadOnlyFault => {
            store.connection.execute_batch("PRAGMA query_only=ON")?;
            return Ok(json!({"fault_injected":true}));
        }
        Control::Register(actors) => {
            store.register(&actors)?;
            let mut catalog = shared.catalog.write().unwrap();
            for actor in actors {
                catalog.people.entry(actor.clone()).or_insert(Person {
                    id: actor.clone(),
                    name: actor.clone(),
                    application_id: None,
                });
                catalog.actors.entry(actor).or_insert(Presence::Online);
            }
            json!({"registered":true})
        }
        Control::Person(person) => {
            store.save_person(&person)?;
            let mut catalog = shared.catalog.write().unwrap();
            catalog
                .actors
                .entry(person.id.clone())
                .or_insert(Presence::Online);
            catalog.people.insert(person.id.clone(), person);
            json!({"saved":true})
        }
        Control::Convert {
            group,
            revision,
            title,
        } => {
            store.convert(&group, revision, &title)?;
            let mut catalog = shared.catalog.write().unwrap();
            let result = catalog.groups.get_mut(&group).context("unknown group")?;
            result.kind = GroupKind::Conversation;
            result.private = false;
            result.title = title;
            result.revision += 1;
            json!(result)
        }
        Control::Fork {
            source,
            group,
            history_from,
        } => {
            store.fork(&source, &group, history_from)?;
            *shared.catalog.write().unwrap() = store.catalog()?;
            json!(shared.catalog.read().unwrap().groups[&group.id])
        }
        Control::Create(group) => {
            let result = store.create_group(&group)?;
            let mut catalog = shared.catalog.write().unwrap();
            catalog
                .members
                .insert(group.id.clone(), group.members.into_iter().collect());
            catalog.groups.insert(group.id, result.clone());
            json!(result)
        }
        Control::Update(change) => {
            store.update_group(&change)?;
            let mut catalog = shared.catalog.write().unwrap();
            let group = catalog
                .groups
                .get_mut(&change.id)
                .context("unknown group after commit")?;
            group.title = change.title;
            group.topic = change.topic;
            group.archived = change.archived;
            group.pinned = change.pinned;
            group.revision += 1;
            json!(group)
        }
        Control::Membership { group, actor, add } => {
            let target = actor.clone();
            store.membership(&group, &actor, add)?;
            let mut catalog = shared.catalog.write().unwrap();
            let members = catalog.members.get_mut(&group).context("unknown group")?;
            if add {
                members.insert(actor);
            } else {
                members.remove(&actor);
            }
            let count = members.len();
            let group = catalog.groups.get_mut(&group).context("unknown group")?;
            group.member_count = count;
            group.revision += 1;
            drop(catalog);
            shared.actor_signal(&target);
            json!({"updated":true})
        }
        Control::Presence { actor, state } => {
            store.set_presence(&actor, state)?;
            shared
                .catalog
                .write()
                .unwrap()
                .actors
                .insert(actor.clone(), state);
            shared.actor_signal(&actor);
            json!({"updated":true})
        }
        Control::Grant {
            actor,
            hash,
            expires,
        } => {
            store.grant(&actor, &hash, expires)?;
            let mut catalog = shared.catalog.write().unwrap();
            catalog
                .grants
                .retain(|_, (id, until)| id != &actor && *until > now_ms());
            catalog.grants.insert(hash, (actor.clone(), expires));
            drop(catalog);
            shared.actor_signal(&actor);
            json!({"issued":true})
        }
        // Reads and acknowledgements do not wake unrelated conversation observers.
        Control::HistoryBefore {
            group,
            before,
            limit,
        } => return Ok(json!(store.history_before(&group, before, limit)?)),
        Control::History {
            group,
            after,
            limit,
        } => return Ok(json!(store.history(&group, after, limit)?)),
        Control::Ack {
            actor,
            group,
            sequence,
        } => {
            store.acknowledge(&actor, &group, sequence)?;
            return Ok(json!({"acknowledged":sequence}));
        }
        Control::Cursor { actor, group } => return Ok(json!(store.cursor(&actor, &group)?)),
    };
    shared.directory_signal();
    Ok(result)
}

// Rejected client constraints roll back normally; only storage/engine failures
// invalidate durable authority and require reopening the database.
fn storage_fault(error: &anyhow::Error) -> bool {
    match error.downcast_ref::<rusqlite::Error>() {
        Some(rusqlite::Error::SqliteFailure(code, _))
            if code.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            false
        }
        Some(rusqlite::Error::QueryReturnedNoRows | rusqlite::Error::ToSqlConversionFailure(_))
        | None => false,
        Some(_) => true,
    }
}
