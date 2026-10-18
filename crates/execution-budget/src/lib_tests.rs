use super::*;
#[test]
fn overlap_and_restart_are_counted_once() {
    let mut b = Budget::default();
    b.tick(100, true).unwrap();
    b.tick(130, true).unwrap();
    let mut b: Budget = serde_json::from_str(&serde_json::to_string(&b).unwrap()).unwrap();
    b.tick(160, false).unwrap();
    b.tick(200, false).unwrap();
    assert_eq!(b.days[&day(100)], 60);
}
#[test]
fn crosses_singapore_midnight_without_reset() {
    let mut b = Budget::default();
    b.tick(57590, true).unwrap();
    b.tick(57610, false).unwrap();
    assert_eq!(b.days[&0], 10);
    assert_eq!(b.days[&1], 10);
    assert!(b.remaining(57610, 5) == 0);
}
#[test]
fn clock_rollback_is_not_free_budget() {
    let mut b = Budget::default();
    b.tick(100, true).unwrap();
    assert!(b.tick(99, true).is_err());
}
