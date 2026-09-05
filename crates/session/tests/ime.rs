use yi_edit_session::ime::{ImeEffect, ImeEvent, ImeState};

#[test]
fn preedit_is_display_only_and_commit_is_the_only_document_effect() {
    let mut state = ImeState::default();
    assert_eq!(state.handle(ImeEvent::Enabled), ImeEffect::None);
    assert_eq!(state.handle(ImeEvent::Preedit("ni".into())), ImeEffect::None);
    assert_eq!(state.preedit(), "ni");
    assert_eq!(state.handle(ImeEvent::Preedit("你".into())), ImeEffect::None);
    assert_eq!(state.preedit(), "你");
    assert_eq!(state.handle(ImeEvent::Commit("你".into())), ImeEffect::Commit("你".into()));
    assert_eq!(state.preedit(), "");
}

#[test]
fn disabled_clears_stale_preedit_and_empty_commit_is_a_noop() {
    let mut state = ImeState::default();
    state.handle(ImeEvent::Preedit("x".into()));
    assert_eq!(state.handle(ImeEvent::Commit(String::new())), ImeEffect::None);
    state.handle(ImeEvent::Preedit("残留".into()));
    assert_eq!(state.handle(ImeEvent::Disabled), ImeEffect::ClearPreedit);
    assert!(!state.enabled());
    assert_eq!(state.preedit(), "");
}
