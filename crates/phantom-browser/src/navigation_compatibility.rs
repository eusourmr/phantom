//! Phantom 2D-4 browser-side navigation compatibility suite.
//!
//! These cases exercise the explicit 2D-3 navigation state machine together
//! with the established 2C fragment/history lifecycle without adding runtime
//! behavior.

use super::*;

fn ready_tab(url: &str) -> BrowserTab {
    let mut tab = BrowserTab::new();
    tab.address = url.to_owned();
    tab.history = vec![url.to_owned()];
    tab.history_scroll_offsets = vec![0.0];
    tab.history_index = Some(0);
    tab.mark_navigation_ready();
    tab
}

#[test]
fn same_document_fragment_navigation_preserves_ready_state_and_pushes_history() {
    let mut tab = ready_tab("https://example.com/page");
    let generation = tab.document_generation;

    let handled =
        try_same_document_fragment_navigation(&mut tab, "#section", NavigationAction::New);

    assert!(handled);
    assert_eq!(tab.navigation_phase(), NavigationPhase::Ready);
    assert_eq!(tab.document_generation, generation);
    assert_eq!(tab.history.len(), 2);
    assert_eq!(tab.history_index, Some(1));
    assert_eq!(tab.address, "https://example.com/page#section");
}

#[test]
fn fragment_history_navigation_restores_saved_scroll_without_fetching() {
    let mut tab = ready_tab("https://example.com/page");
    tab.history = vec![
        "https://example.com/page".to_owned(),
        "https://example.com/page#section".to_owned(),
    ];
    tab.history_scroll_offsets = vec![12.0, 96.0];
    tab.history_index = Some(0);
    tab.current_scroll_y = 12.0;

    let handled = try_same_document_fragment_navigation(
        &mut tab,
        "https://example.com/page#section",
        NavigationAction::History(1),
    );

    assert!(handled);
    assert_eq!(tab.navigation_phase(), NavigationPhase::Ready);
    assert_eq!(tab.history_index, Some(1));
    assert_eq!(tab.current_scroll_y, 96.0);
    assert_eq!(tab.pending_scroll_y, Some(96.0));
}

#[test]
fn reload_does_not_take_same_document_fragment_shortcut() {
    let mut tab = ready_tab("https://example.com/page#section");

    let handled = try_same_document_fragment_navigation(
        &mut tab,
        "https://example.com/page#section",
        NavigationAction::Reload,
    );

    assert!(!handled);
    assert_eq!(tab.navigation_phase(), NavigationPhase::Ready);
    assert_eq!(tab.history.len(), 1);
}

#[test]
fn redirected_commit_restores_requested_fragment_when_response_has_none() {
    let committed = navigation_commit_url(
        "https://example.com/final",
        "https://example.com/start#chapter",
    );

    assert_eq!(committed, "https://example.com/final#chapter");
}

#[test]
fn response_fragment_wins_over_requested_fragment() {
    let committed = navigation_commit_url(
        "https://example.com/final#server",
        "https://example.com/start#client",
    );

    assert_eq!(committed, "https://example.com/final#server");
}

#[test]
fn new_history_commit_discards_forward_branch() {
    let mut tab = ready_tab("https://example.com/b");
    tab.history = vec![
        "https://example.com/a".to_owned(),
        "https://example.com/b".to_owned(),
        "https://example.com/c".to_owned(),
    ];
    tab.history_scroll_offsets = vec![1.0, 2.0, 3.0];
    tab.history_index = Some(1);

    commit_history(&mut tab, NavigationAction::New, "https://example.com/d");

    assert_eq!(
        tab.history,
        vec![
            "https://example.com/a".to_owned(),
            "https://example.com/b".to_owned(),
            "https://example.com/d".to_owned(),
        ]
    );
    assert_eq!(tab.history_scroll_offsets, vec![1.0, 2.0, 0.0]);
    assert_eq!(tab.history_index, Some(2));
}

#[test]
fn history_commit_updates_only_target_entry() {
    let mut tab = ready_tab("https://example.com/b");
    tab.history = vec![
        "https://example.com/a".to_owned(),
        "https://example.com/b".to_owned(),
    ];
    tab.history_scroll_offsets = vec![14.0, 28.0];
    tab.history_index = Some(1);

    commit_history(
        &mut tab,
        NavigationAction::History(0),
        "https://example.com/a-final",
    );

    assert_eq!(
        tab.history,
        vec![
            "https://example.com/a-final".to_owned(),
            "https://example.com/b".to_owned(),
        ]
    );
    assert_eq!(tab.history_scroll_offsets, vec![14.0, 28.0]);
    assert_eq!(tab.history_index, Some(0));
}

#[test]
fn reload_commit_preserves_history_shape() {
    let mut tab = ready_tab("https://example.com/page");
    tab.history_scroll_offsets = vec![44.0];

    commit_history(
        &mut tab,
        NavigationAction::Reload,
        "https://example.com/page?fresh=1",
    );

    assert_eq!(
        tab.history,
        vec!["https://example.com/page?fresh=1".to_owned()]
    );
    assert_eq!(tab.history_scroll_offsets, vec![44.0]);
    assert_eq!(tab.history_index, Some(0));
}

#[test]
fn stale_fetch_generation_is_discarded_defensively() {
    let (_sender, receiver) = mpsc::channel::<Result<DocumentResponse, DocumentLoadError>>();
    let mut tab = BrowserTab::new();
    tab.document_generation = 8;
    tab.begin_fetching(PendingNavigation {
        receiver,
        action: NavigationAction::New,
        generation: 7,
    });

    let network = NetworkClient::new();
    poll_tab_navigation(&mut tab, &network, 1.0);

    assert_eq!(tab.navigation_phase(), NavigationPhase::Empty);
    assert!(!tab.is_loading());
}

#[test]
fn disconnected_fetch_transitions_to_failed() {
    let (sender, receiver) = mpsc::channel::<Result<DocumentResponse, DocumentLoadError>>();
    drop(sender);

    let mut tab = BrowserTab::new();
    tab.document_generation = 3;
    tab.begin_fetching(PendingNavigation {
        receiver,
        action: NavigationAction::New,
        generation: 3,
    });

    let network = NetworkClient::new();
    poll_tab_navigation(&mut tab, &network, 1.0);

    assert_eq!(tab.navigation_phase(), NavigationPhase::Failed);
    assert!(
        tab.navigation_error()
            .is_some_and(|error| { error.title == "Não foi possível abrir esta página" })
    );
}

#[test]
fn empty_receiver_keeps_fetching_state() {
    let (_sender, receiver) = mpsc::channel::<Result<DocumentResponse, DocumentLoadError>>();

    let mut tab = BrowserTab::new();
    tab.document_generation = 5;
    tab.begin_fetching(PendingNavigation {
        receiver,
        action: NavigationAction::Reload,
        generation: 5,
    });

    let network = NetworkClient::new();
    poll_tab_navigation(&mut tab, &network, 1.0);

    assert_eq!(tab.navigation_phase(), NavigationPhase::Fetching);
    assert_eq!(tab.loading_action(), Some(NavigationAction::Reload));
}

#[test]
fn failed_navigation_can_be_replaced_by_new_fetch() {
    let mut tab = BrowserTab::new();
    tab.fail_navigation(DocumentPageError::new("Falha anterior", "erro controlado"));

    let (_sender, receiver) = mpsc::channel::<Result<DocumentResponse, DocumentLoadError>>();
    tab.begin_fetching(PendingNavigation {
        receiver,
        action: NavigationAction::New,
        generation: 1,
    });

    assert_eq!(tab.navigation_phase(), NavigationPhase::Fetching);
    assert!(tab.navigation_error().is_none());
}

#[test]
fn cross_document_target_is_not_mistaken_for_fragment_navigation() {
    let mut tab = ready_tab("https://example.com/page");

    let handled = try_same_document_fragment_navigation(
        &mut tab,
        "https://other.example/page#section",
        NavigationAction::New,
    );

    assert!(!handled);
    assert_eq!(tab.navigation_phase(), NavigationPhase::Ready);
    assert_eq!(tab.history.len(), 1);
}
