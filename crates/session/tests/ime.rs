use yi_edit_session::ime::{ImeEffect, ImeEvent, ImeState};

#[test]
fn preedit_is_display_only_and_commit_is_the_only_document_effect() {
    let mut ime = ImeState::default();
    assert_eq!(ime.handle(ImeEvent::Enabled), ImeEffect::None);
    assert_eq!(ime.handle(ImeEvent::Preedit("ni".into())), ImeEffect::None);
    assert_eq!(ime.preedit(), "ni");
    assert_eq!(ime.handle(ImeEvent::Preedit("你".into())), ImeEffect::None);
    assert_eq!(ime.preedit(), "你");
    assert_eq!(ime.handle(ImeEvent::Commit("你".into())), ImeEffect::Commit("你".into()));
    assert_eq!(ime.preedit(), "");
}

#[test]
fn disabled_clears_stale_preedit_and_empty_commit_is_a_noop() {
    let mut ime = ImeState::default();
    ime.handle(ImeEvent::Preedit("x".into()));
    assert_eq!(ime.handle(ImeEvent::Commit(String::new())), ImeEffect::None);
    ime.handle(ImeEvent::Preedit("残留".into()));
    assert_eq!(ime.handle(ImeEvent::Disabled), ImeEffect::ClearPreedit);
    assert!(!ime.enabled());
    assert_eq!(ime.preedit(), "");
}
