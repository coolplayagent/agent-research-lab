use super::*;
#[test]
fn retirement_erases_nonempty_feature_tables_and_preserves_unrelated_rows() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("messages.sqlite");
    let db = Connection::open(&path).unwrap();
    fixture_version(&db, 8).unwrap();
    db.execute_batch("INSERT INTO actors(id) VALUES('operator');
        INSERT INTO groups(id,title,topic,private,created_ms) VALUES('room','Room','Topic',0,1);
        INSERT INTO members(group_id,actor_id) VALUES('room','operator');
        INSERT INTO messages(group_id,sender_id,request_id,text,accepted_ms) VALUES('room','operator','keep','Ordinary discussion',1);
        INSERT INTO research_topics VALUES('topic','room',1,'{}');
        INSERT INTO research_subjects VALUES('subject','topic',1,'{}');
        INSERT INTO research_nodes(id,subject_id,value) VALUES('base','subject','private-evidence');
        INSERT INTO research_nodes(id,subject_id,value) VALUES('next','subject','{}');
        INSERT INTO research_edges VALUES('next','base');
        INSERT INTO evolution_runs VALUES('run','room','design-review',1,'{}');
        INSERT INTO evolution_requests VALUES('run','request','digest');").unwrap();
    crate::retirement::prepare(&db).unwrap();
    let preserved = columns(&db)
        .unwrap()
        .into_iter()
        .filter(|(name, _)| !RETIRED_TABLES.contains(&name.as_str()) && name != "outcomes")
        .collect();
    let before = rows(&db, &preserved).unwrap();
    drop(db);
    convert(&path).unwrap();
    validate(&path).unwrap();
    let db = Connection::open(&path).unwrap();
    crate::retirement::prepare(&db).unwrap();
    assert_eq!(before, rows(&db, &preserved).unwrap());
    for name in RETIRED_TABLES {
        assert!(db.prepare(&format!("SELECT * FROM {name}")).is_err());
    }
    drop(db);
    convert(&path).unwrap();
    validate(&path).unwrap();
}

#[test]
fn retirement_removes_all_round_goals_and_results_but_keeps_chat_reply_identity() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("messages.sqlite");
    let db = Connection::open(&path).unwrap();
    fixture_version(&db, 8).unwrap();
    db.execute_batch(r#"INSERT INTO actors(id) VALUES('operator');
        INSERT INTO groups(id,title,topic,private,created_ms,last_sequence) VALUES('room','Room','Topic',0,1,3);
        INSERT INTO members VALUES('room','operator',3);
        INSERT INTO groups(id,title,topic,private,created_ms) VALUES('independent','Independent','Independent',0,1);
        INSERT INTO goals VALUES('ordinary','independent','collaboration',1,'unchanged ordinary result');
        INSERT INTO goals VALUES('round-one','room','design-review',0,'first round result');
        INSERT INTO goals VALUES('round-two','room','design-review',1,'current round result');
        INSERT INTO goals VALUES('software','room','software-evolution',1,'retired software result');
        INSERT INTO goal_assignees VALUES('round-one','operator'),('round-two','operator'),('ordinary','operator');
        INSERT INTO evolution_runs VALUES('run','room','design-review',1,'retired run evidence');
        INSERT INTO evolution_goal_bindings VALUES('round-one','run',1),('round-two','run',2);
        INSERT INTO messages(sequence,group_id,sender_id,request_id,text,event,accepted_ms) VALUES(1,'room','operator','result','Retired submitted body','{"goal_id":"round-one","result":{"private":"evidence"}}',1);
        INSERT INTO messages(sequence,group_id,sender_id,request_id,text,reply_to,accepted_ms) VALUES(2,'room','operator','chat','Ordinary reply',1,1);
        INSERT INTO messages(sequence,group_id,sender_id,request_id,text,event,accepted_ms) VALUES(3,'room','operator','normal','Ordinary submission','{"goal_id":"ordinary"}',1);
        INSERT INTO research_topics VALUES('topic','room',1,'{}');
        INSERT INTO outcome_paths VALUES('topic','round-one','retired path');
        INSERT INTO outcomes(id,topic_id,goal_id,author_id,kind,title,summary,value) VALUES('result','topic','round-one','operator','conclusion','Retired','Retired','retired outcome evidence');
        INSERT INTO outcome_events(outcome_id,kind,request_id,value,created_ms) VALUES('result','acceptance','accept','retired acceptance',1);
    "#).unwrap();
    drop(db);
    convert(&path).unwrap();
    let db = Connection::open(&path).unwrap();
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM goals", [], |r| r.get::<_, u64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row("SELECT value FROM goals", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "unchanged ordinary result"
    );
    for table in ["outcomes", "outcome_events"] {
        assert_eq!(
            db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                .get::<_, u64>(0))
                .unwrap(),
            0
        );
    }
    assert_eq!(
        db.query_row(
            "SELECT text,event FROM messages WHERE sequence=1",
            [],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        )
        .unwrap(),
        (String::new(), None)
    );
    assert_eq!(
        db.query_row(
            "SELECT text,reply_to FROM messages WHERE sequence=2",
            [],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, u64>(1)?))
        )
        .unwrap(),
        ("Ordinary reply".into(), 1)
    );
    assert_eq!(
        db.query_row("SELECT text FROM messages WHERE sequence=3", [], |r| r
            .get::<_, String>(
            0
        ))
        .unwrap(),
        "Ordinary submission"
    );
    assert_eq!(
        db.query_row("SELECT cursor FROM members", [], |r| r.get::<_, u64>(0))
            .unwrap(),
        3
    );
    assert_eq!(
        db.query_row("SELECT last_sequence FROM groups", [], |r| r
            .get::<_, u64>(0))
            .unwrap(),
        3
    );
}
