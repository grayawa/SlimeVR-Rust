use serde_json::json;
use slimevr_gpui::checklist::{Status, derive};
use std::collections::BTreeSet;
#[test]
fn disabled_ignored_and_completed_conditional_steps_are_hidden() {
    let value = json!({"steps":[
        {"id":1,"enabled":false,"valid":false},
        {"id":2,"enabled":true,"valid":false},
        {"id":3,"enabled":true,"valid":true,"visibility":1},
        {"id":4,"enabled":true,"valid":false},
        {"id":5,"enabled":true,"valid":true,"visibility":0}],"ignored_steps":[2]});
    let checklist = derive(&value, &BTreeSet::from([4]));
    assert_eq!(
        checklist.steps.iter().map(|s| s.id).collect::<Vec<_>>(),
        [5]
    );
    assert_eq!(checklist.completion, "complete");
    assert_eq!(checklist.progress, 1.);
    assert_eq!(value["ignored_steps"], json!([2]));
}
#[test]
fn required_steps_block_later_actions_and_optional_steps_do_not() {
    let value = json!({"steps":[
        {"id":1,"enabled":true,"valid":false,"optional":true},
        {"id":2,"enabled":true,"valid":false},
        {"id":3,"enabled":true,"valid":false},
        {"id":4,"enabled":true,"valid":true}]});
    let checklist = derive(&value, &BTreeSet::new());
    assert_eq!(
        checklist.steps.iter().map(|s| s.status).collect::<Vec<_>>(),
        [
            Status::Skipped,
            Status::Invalid,
            Status::Blocked,
            Status::Complete
        ]
    );
    assert!(checklist.steps[0].first_required);
    let skipped = derive(&value, &BTreeSet::from([2]));
    assert_eq!(skipped.steps[1].status, Status::Invalid);
    assert_eq!(
        derive(&value, &BTreeSet::from([2, 3])).completion,
        "partial"
    );
}
