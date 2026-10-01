//! apim 侧导入记录文件（`store`）的测试。

use crate::clients::codex::store;

use super::helpers::{state_for, temp_dir};

#[test]
fn state_roundtrip_and_key_id() {
    let dir = temp_dir("state");
    let state = state_for("ikun", "codex", &["a", "b"]);
    store::save(&dir, &state).unwrap();
    assert_eq!(store::load(&dir), Some(state.clone()));
    assert_eq!(state.key_id(), "ikun.codex");
}

#[test]
fn missing_state_loads_as_none() {
    let dir = temp_dir("state-missing");
    assert_eq!(store::load(&dir), None);
}
