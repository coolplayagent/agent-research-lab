use super::*;

fn available(db: &Connection, actor: &str, id: &str) -> Result<OutcomeView> {
    let entry = outcome(db, id)?;
    authorize(db, actor, &entry.input.topic_id, true)?;
    check(current_attempt(db, &entry)?, Code::StaleAttempt)?;
    let view = view(db, entry)?;
    check(
        view.publication == OutcomePublicationState::Published,
        Code::Conflict,
    )?;
    Ok(view)
}

pub(super) fn write_result(db: &Connection, id: &str) -> Result<Option<KnowledgeWriteResult>> {
    Ok(by_request(db, Event::WriteReconciled, id)?.or(by_request(db, Event::WriteFinished, id)?))
}

pub(super) fn change(db: &Connection, actor: &str, command: OutcomeCommand) -> Result<Value> {
    match command {
        OutcomeCommand::BeginVerification { input } => {
            id(&input.id)?;
            id(&input.verifier_id)?;
            hash(&input.outcome_digest, &[64])?;
            hash(&input.verifier_digest, &[64])?;
            check((1..=600).contains(&input.max_seconds), Code::Invalid)?;
            let view = available(db, actor, &input.outcome_id)?;
            check(
                view.outcome.digest == input.outcome_digest,
                Code::BindingChanged,
            )?;
            if let Some(mut existing) =
                by_request::<OutcomeVerification>(db, Event::VerificationStarted, &input.id)?
            {
                check(existing.input == input, Code::Conflict)?;
                existing.result = by_request(db, Event::VerificationFinished, &input.id)?;
                return Ok(json!({"record":existing,"created":false}));
            }
            check(
                view.verification != OutcomeVerificationState::Running,
                Code::Conflict,
            )?;
            let created_ms = now_ms();
            let run = OutcomeVerification {
                request_digest: digest(&input)?,
                deadline_ms: created_ms + input.max_seconds * 1000,
                input,
                created_ms,
                result: None,
            };
            event(
                db,
                &run.input.outcome_id,
                Event::VerificationStarted,
                &run.input.id,
                &run,
            )?;
            Ok(json!({"record":run,"created":true}))
        }
        OutcomeCommand::FinishVerification { result } => {
            let run =
                by_request::<OutcomeVerification>(db, Event::VerificationStarted, &result.id)?
                    .context(Code::NotFound)?;
            let entry = outcome(db, &run.input.outcome_id)?;
            authorize(db, actor, &entry.input.topic_id, false)?;
            check(
                result.request_digest == run.request_digest,
                Code::BindingChanged,
            )?;
            check(result.state != VerificationRunState::Running, Code::Invalid)?;
            bounded(&result.summary, 4096)?;
            if let Some(old) = by_request::<OutcomeVerificationResult>(
                db,
                Event::VerificationFinished,
                &result.id,
            )? {
                check(old == result, Code::Conflict)?;
                return Ok(json!(view(db, entry)?));
            }
            if matches!(
                result.state,
                VerificationRunState::Passed | VerificationRunState::Failed
            ) {
                validate_evidence(&result.evidence)?;
                let current = available(db, actor, &run.input.outcome_id)?;
                check(
                    current
                        .latest_verification
                        .as_ref()
                        .is_some_and(|latest| latest.input.id == result.id),
                    Code::StaleAttempt,
                )?;
                check(run.deadline_ms > now_ms(), Code::StaleAttempt)?;
            } else {
                check(result.evidence.is_empty(), Code::Invalid)?;
            }
            event(
                db,
                &run.input.outcome_id,
                Event::VerificationFinished,
                &result.id,
                &result,
            )?;
            Ok(json!(view(db, entry)?))
        }
        OutcomeCommand::Decide { decision } => {
            id(&decision.id)?;
            bounded(&decision.reason, 4096)?;
            check(
                decision.state != OutcomeAcceptanceState::Pending,
                Code::Invalid,
            )?;
            let view = available(db, actor, &decision.outcome_id)?;
            if let Some(old) = by_request::<OutcomeDecision>(db, Event::Decision, &decision.id)? {
                check(old == decision, Code::Conflict)?;
                return Ok(json!(view));
            }
            if decision.state == OutcomeAcceptanceState::Accepted {
                check(
                    view.verification == OutcomeVerificationState::Passed
                        && view.latest_verification.as_ref().is_some_and(|run| {
                            decision.verification_id.as_ref() == Some(&run.input.id)
                        }),
                    Code::VerificationRequired,
                )?;
            }
            event(
                db,
                &decision.outcome_id,
                Event::Decision,
                &decision.id,
                &decision,
            )?;
            Ok(json!(super::view(db, view.outcome)?))
        }
        OutcomeCommand::BeginWrite { input } => {
            id(&input.id)?;
            id(&input.service_id)?;
            id(&input.acceptance_id)?;
            hash(&input.outcome_digest, &[64])?;
            hash(&input.service_digest, &[64])?;
            bounded(&input.repository, 1024)?;
            bounded(&input.expected_revision, 256)?;
            check((1..=600).contains(&input.max_seconds), Code::Invalid)?;
            let view = available(db, actor, &input.outcome_id)?;
            check(
                view.outcome.digest == input.outcome_digest,
                Code::BindingChanged,
            )?;
            if let Some(mut existing) =
                by_request::<KnowledgeWrite>(db, Event::WriteStarted, &input.id)?
            {
                check(existing.input == input, Code::Conflict)?;
                existing.result = write_result(db, &input.id)?;
                return Ok(json!({"record":existing,"created":false}));
            }
            check(
                view.acceptance == OutcomeAcceptanceState::Accepted
                    && view
                        .latest_decision
                        .as_ref()
                        .is_some_and(|decision| decision.id == input.acceptance_id),
                Code::VerificationRequired,
            )?;
            check(
                view.latest_write.as_ref().is_none_or(|write| {
                    write.result.as_ref().is_some_and(|result| {
                        matches!(
                            result.state,
                            KnowledgeWriteState::Applied | KnowledgeWriteState::Failed
                        )
                    })
                }),
                Code::Conflict,
            )?;
            let created_ms = now_ms();
            let write = KnowledgeWrite {
                request_digest: digest(&input)?,
                deadline_ms: created_ms + input.max_seconds * 1000,
                input,
                created_ms,
                result: None,
            };
            event(
                db,
                &write.input.outcome_id,
                Event::WriteStarted,
                &write.input.id,
                &write,
            )?;
            Ok(json!({"record":write,"created":true}))
        }
        OutcomeCommand::FinishWrite { result } => {
            let write = by_request::<KnowledgeWrite>(db, Event::WriteStarted, &result.id)?
                .context(Code::NotFound)?;
            let entry = outcome(db, &write.input.outcome_id)?;
            // Reconciliation must remain possible after cancellation or archival.
            authorize(db, actor, &entry.input.topic_id, false)?;
            check(
                result.request_digest == write.request_digest,
                Code::BindingChanged,
            )?;
            bounded(&result.summary, 4096)?;
            check(result.state != KnowledgeWriteState::Pending, Code::Invalid)?;
            if result.state == KnowledgeWriteState::Applied {
                bounded(&result.reference, 2048)?;
                bounded(&result.revision, 256)?;
            } else {
                check(
                    result.reference.is_empty() && result.revision.is_empty(),
                    Code::Invalid,
                )?;
            }
            let kind = if let Some(old) = write_result(db, &result.id)? {
                if old == result {
                    return Ok(json!(view(db, entry)?));
                }
                check(
                    old.state == KnowledgeWriteState::Unknown
                        && result.state != KnowledgeWriteState::Unknown,
                    Code::Conflict,
                )?;
                Event::WriteReconciled
            } else {
                Event::WriteFinished
            };
            event(db, &write.input.outcome_id, kind, &result.id, &result)?;
            Ok(json!(view(db, entry)?))
        }
        _ => anyhow::bail!(Code::Invalid),
    }
}
