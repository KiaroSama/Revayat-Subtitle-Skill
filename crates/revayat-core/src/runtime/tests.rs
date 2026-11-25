use super::*;
use serde_json::json;

#[test]
fn identical_json_requires_identical_number_types() {
    assert_eq!(
        same_json(json!({"a":1,"b":true}), json!({"b":true,"a":1})),
        Ok(true)
    );
    assert_eq!(same_json(json!({"a":1}), json!({"a":1.0})), Ok(false));
    assert_eq!(same_json(json!(true), json!(1)), Ok(false));
}
