use super::*;
#[test]
fn independent_outcome_references_block_erasure_before_any_durable_write() {
    let db = Connection::open_in_memory().unwrap();
    crate::database::fixture_version(&db, 8).unwrap();
    db.execute_batch("PRAGMA foreign_keys=OFF;
        INSERT INTO evolution_goal_bindings VALUES('iteration-goal','run',1);
        INSERT INTO outcomes(id,topic_id,goal_id,author_id,kind,title,summary,value) VALUES('retired','topic','iteration-goal','operator','conclusion','Title','Summary','{}');
        INSERT INTO outcome_references VALUES('independent','retired','supports');").unwrap();
    assert!(
        prepare(&db)
            .unwrap_err()
            .to_string()
            .contains("independent outcome")
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM outcomes", [], |r| r.get::<_, u64>(0))
            .unwrap(),
        1
    );
    assert!(exists(&db, "evolution_goal_bindings").unwrap());
}
