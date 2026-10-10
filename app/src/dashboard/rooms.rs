//! Operator control surface; stable people are registered by the authoritative directory.
use super::transport::{Web, body, boundary, error, intent, json_response};
use super::*;
use axum::{
    extract::{Query, Request, State},
    http::HeaderMap,
    response::{
        IntoResponse, Response, Sse,
        sse::{Event, KeepAlive},
    },
};
use serde::Deserialize;
use std::convert::Infallible;
#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
enum Change {
    Create {
        group: crystal::NewGroup,
    },
    Update {
        change: crystal::GroupChange,
    },
    Membership {
        group_id: String,
        person_id: String,
        add: bool,
    },
    Presence {
        person_id: String,
        state: crystal::Presence,
    },
    Grant {
        person_id: String,
        lifetime_seconds: u64,
    },
}
async fn register(web: &Web) -> Result<()> {
    let config = web
        .config
        .clone()
        .context("identity directory unavailable")?;
    let people = tokio::task::spawn_blocking(move || runtime::people::directory(&config)).await??;
    web.hub
        .register(people.people.keys().cloned().collect())
        .await
}
pub(super) async fn manage(State(web): State<Web>, request: Request) -> Response {
    let result=async{
        intent(request.headers(),web.addr,"manage-crystal")?;
        let _permit=web.reads.clone().try_acquire_owned().context("management backpressure")?;
        let change:Change=serde_json::from_slice(&body(request,512*1024).await?)?;
        // This is a host control boundary, never accepted from a coding-agent grant.
        register(&web).await?;
        match change {
            Change::Create{group}=>Ok(json!(web.hub.create_group(group).await?)),
            Change::Update{change}=>{web.hub.update_group(change).await?;Ok(json!({"updated":true}))},
            Change::Membership{group_id,person_id,add}=>{web.hub.membership(&group_id,&person_id,add).await?;Ok(json!({"updated":true}))},
            Change::Presence{person_id,state}=>{web.hub.set_presence(&person_id,state).await?;Ok(json!(web.hub.presence(&person_id)?))},
            Change::Grant{person_id,lifetime_seconds}=>Ok(json!({"token":web.hub.grant(&person_id,lifetime_seconds).await?,"person_id":person_id,"expires_in_seconds":lifetime_seconds})),
        }
    }.await;
    json_response(result)
}
#[derive(Deserialize, Default)]
pub(super) struct View {
    #[serde(default)]
    group_id: String,
    #[serde(default)]
    after: String,
    #[serde(default)]
    query: String,
    #[serde(default)]
    sequence: u64,
    before: Option<u64>,
}
// Existing evidence-bound run boards remain behind the same observation API.
// Their bounded protocol is immutable; it is never replayed into a new agent inbox.
fn frozen_catalog(web: &Web) -> Value {
    let snapshot = web.snapshot.read().unwrap();
    let mut groups = BTreeMap::<String, Value>::new();
    for job in snapshot["jobs"].as_array().into_iter().flatten() {
        if let Some(id) = job["cohort_id"].as_str() {
            groups.entry(id.into()).or_insert_with(||json!({"id":format!("run_{id}"),"cohort_id":id,"title":job["team"],"topics":job["topics"],"kind":"frozen_run","managed_by":"research_controller"}));
        }
    }
    json!(groups.into_values().collect::<Vec<_>>())
}
fn frozen_view(web: &Web, id: &str) -> Result<Value> {
    ensure!(
        id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid frozen run channel"
    );
    let snapshot = web.snapshot.read().unwrap();
    let board = snapshot["boards"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|board| board["cohort_id"] == id)
        .context("frozen channel unavailable in current observer window")?;
    Ok(
        json!({"kind":"frozen_run","id":format!("run_{id}"),"board":board,"authority":"host_frozen_experiment_protocol","replay_to_new_conversation":false}),
    )
}

pub(super) async fn view(
    State(web): State<Web>,
    headers: HeaderMap,
    Query(query): Query<View>,
) -> Response {
    let result=async{boundary(&headers,web.addr)?;
        if let Some(id)=query.group_id.strip_prefix("run_"){return frozen_view(&web,id);}
        if query.group_id.is_empty(){return Ok(json!({"metrics":web.hub.metrics(),"directory":web.hub.groups(&query.after,&query.query,100)?,"frozen_groups":frozen_catalog(&web)}));}
        Ok(json!({"group":web.hub.group(&query.group_id)?,"members":web.hub.members(&query.group_id,&query.after,100)?,"messages":if query.before.is_some() || query.sequence==0 { web.hub.history_before(&query.group_id, query.before.unwrap_or(i64::MAX as u64),128).await? } else { web.hub.history(&query.group_id,query.sequence,128).await? }}))
    }.await;
    json_response(result)
}
pub(super) async fn events(
    State(web): State<Web>,
    headers: HeaderMap,
    Query(query): Query<View>,
) -> Response {
    if let Err(e) = boundary(&headers, web.addr) {
        return error(e);
    }
    let permit = match web.reads.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(e) => return error(format!("observer backpressure: {e}")),
    };
    if !query.group_id.is_empty() && web.hub.group(&query.group_id).is_err() {
        return error("unknown conversation");
    }
    let mut changes = web.hub.changes();
    let mut stop = web.shutdown.subscribe();
    let mut cursor = match headers
        .get("last-event-id")
        .map(|v| v.to_str().ok().and_then(|s| s.parse::<u64>().ok()))
    {
        Some(Some(value)) => value.max(query.sequence),
        Some(None) => return error("invalid replay cursor"),
        None => query.sequence,
    };
    let stream = async_stream::stream! {
        let _permit=permit;
        loop {
            if *stop.borrow(){break;}
            let group=(!query.group_id.is_empty()).then(||web.hub.group(&query.group_id).ok()).flatten();
            let value=json!({"metrics":web.hub.metrics(),"group":group});
            yield Ok::<_,Infallible>(Event::default().event("status").json_data(value).unwrap());
            if !query.group_id.is_empty(){
                loop{
                    match web.hub.history(&query.group_id,cursor,128).await {
                        Ok(page)=>{let len=page.len();for message in page{cursor=message.sequence;yield Ok(Event::default().event("message").id(cursor.to_string()).json_data(message).unwrap());}if len<128{break;}}
                        Err(_)=>{yield Ok(Event::default().event("fault").data("History unavailable"));break;}
                    }
                }
            }
            tokio::select!{_=stop.changed()=>break,_=changes.changed()=>{}}
            // Coalesce a burst of notifications, never periodically fetch state.
            tokio::select!{_=stop.changed()=>break,_=tokio::time::sleep(Duration::from_millis(50))=>{}}
        }
    };
    Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
        .into_response()
}
