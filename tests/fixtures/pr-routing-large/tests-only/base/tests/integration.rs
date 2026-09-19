use crate::CASES;

#[test]
fn case_01_is_registered() {
    let ids: Vec<u32> = CASES.iter().map(|case| case.id).collect();
    assert!(ids.contains(&1), "case 1 must stay registered");
}

#[test]
fn case_02_is_registered() {
    let ids: Vec<u32> = CASES.iter().map(|case| case.id).collect();
    assert!(ids.contains(&2), "case 2 must stay registered");
}

