use yi_edit_session::ime::{ImeEffect, ImeEvent, ImeState};

#[test]
fn preedit_is_display_only_and_commit_is_the_only_document_effect() {
    let mut state = ImeState::default();
    assert_eq!(state.handle(ImeEvent::Enabled), ImeEffect::None);
    assert_eq!(
        state.handle(ImeEvent::Preedit { text: "ni".into(), active_range_chars: Some(0..2) }),
        ImeEffect::None
    );
    assert_eq!(state.preedit(), "ni");
    assert_eq!(state.active_range_chars(), Some(0..2));
    assert_eq!(
        state.handle(ImeEvent::Preedit { text: "你".into(), active_range_chars: Some(0..1) }),
        ImeEffect::None
    );
    assert_eq!(state.handle(ImeEvent::Commit("你".into())), ImeEffect::Commit("你".into()));
    assert_eq!(state.preedit(), "");
    assert_eq!(state.active_range_chars(), None);
}

#[test]
fn disabled_clears_stale_preedit_and_empty_commit_is_a_noop() {
    let mut state = ImeState::default();
    state.handle(ImeEvent::Preedit { text: "x".into(), active_range_chars: None });
    assert_eq!(state.handle(ImeEvent::Commit(String::new())), ImeEffect::None);
    state.handle(ImeEvent::Preedit { text: "残留".into(), active_range_chars: Some(0..2) });
    assert_eq!(state.handle(ImeEvent::Disabled), ImeEffect::ClearPreedit);
    assert!(!state.enabled());
    assert_eq!(state.preedit(), "");
    assert_eq!(state.active_range_chars(), None);
}
